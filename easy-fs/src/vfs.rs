use super::{
    block_cache_sync_all, get_block_cache, BlockDevice, DirEntry, DiskInode, DiskInodeType,
    EasyFileSystem, DIRENT_SZ,
};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
// use core::cell::RefCell;
use spin::{Mutex, MutexGuard};
/// Virtual filesystem layer over easy-fs
pub struct Inode {
    block_id: usize,
    block_offset: usize,
    fs: Arc<Mutex<EasyFileSystem>>,
    block_device: Arc<dyn BlockDevice>,
    inode_id: Arc<Mutex<u64>>,
}

impl Inode {
    /// Create a vfs inode
    pub fn new(
        block_id: u32,
        block_offset: usize,
        fs: Arc<Mutex<EasyFileSystem>>,
        block_device: Arc<dyn BlockDevice>,
    ) -> Self {
        Self {
            block_id: block_id as usize,
            block_offset,
            fs,
            block_device,
            inode_id: Arc::new(Mutex::new(180)),
        }
    }
    /// Call a function over a disk inode to read it
    fn read_disk_inode<V>(&self, f: impl FnOnce(&DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .read(self.block_offset, f)
    }
    /// Call a function over a disk inode to modify it
    fn modify_disk_inode<V>(&self, f: impl FnOnce(&mut DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .modify(self.block_offset, f)
    }
    /// Find inode under a disk inode by name
    fn find_inode_id(&self, name: &str, disk_inode: &DiskInode) -> Option<u32> {
        // assert it is a directory
        assert!(disk_inode.is_dir());
        let file_count = (disk_inode.size as usize) / DIRENT_SZ;
        let mut dirent = DirEntry::empty();
        for i in 0..file_count {
            assert_eq!(
                disk_inode.read_at(DIRENT_SZ * i, dirent.as_bytes_mut(), &self.block_device,),
                DIRENT_SZ,
            );
            if dirent.name() == name {
                return Some(dirent.inode_id());
            }
        }
        None
    }
    /// Find inode under current inode by name
    pub fn find(&self, name: &str) -> Option<Arc<Inode>> {
        let fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            self.find_inode_id(name, disk_inode).map(|inode_id| {
                let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);
                let new_inode = Self::new(
                    block_id,
                    block_offset,
                    self.fs.clone(),
                    self.block_device.clone(),
                );
                {
                    let mut new_inode_id = new_inode.inode_id.lock();
                    *new_inode_id = inode_id as u64;
                }
                Arc::new(new_inode)
                // Arc::new(Self::new(
                //     block_id,
                //     block_offset,
                //     self.fs.clone(),
                //     self.block_device.clone(),
                // ))
            })
        })
    }
    /// Increase the size of a disk inode
    fn increase_size(
        &self,
        new_size: u32,
        disk_inode: &mut DiskInode,
        fs: &mut MutexGuard<EasyFileSystem>,
    ) {
        if new_size < disk_inode.size {
            return;
        }
        let blocks_needed = disk_inode.blocks_num_needed(new_size);
        let mut v: Vec<u32> = Vec::new();
        for _ in 0..blocks_needed {
            v.push(fs.alloc_data());
        }
        disk_inode.increase_size(new_size, v, &self.block_device);
    }

    ///返回硬连接数(只能root_inode使用)
    pub fn get_links(&self, inode_id: u32) -> u32 {
        log::debug!("into get_links");
        let mut nlink = 0;
        self.read_disk_inode(|root_disk_inode| {
            let file_count = (root_disk_inode.size as usize) / DIRENT_SZ;
            let mut dirent = DirEntry::empty();
            for i in 0..file_count {
                log::debug!("i is {}", i);
                assert_eq!(
                    root_disk_inode.read_at(
                        DIRENT_SZ * i,
                        dirent.as_bytes_mut(),
                        &self.block_device,
                    ),
                    DIRENT_SZ,
                );
                if dirent.inode_id() == inode_id {
                    nlink += 1;
                }
            }
            log::debug!("exit loop");
        });
        log::debug!("exit read_disk_inode");
        nlink
    }

    /// 返回元数据
    pub fn get_metadata(&self) -> (u64, u32) {
        log::debug!("into get_metadata");
        log::debug!(
            "block id is {}, offset is {}",
            self.block_id,
            self.block_offset
        );
        let mut mode: u32 = 0;
        self.read_disk_inode(|disk_node| {
            if disk_node.is_dir() {
                mode = 0o040000;
            } else {
                mode = 0o100000;
            }
        });
        let inode_id = *self.inode_id.lock();
        log::debug!("metadata indoe id is {}", inode_id);
        (inode_id, mode)
    }
    /// 添加硬连接
    pub fn add_link(&self, old_name: &str, new_name: &str) -> isize {
        let mut fs = self.fs.lock();
        self.modify_disk_inode(|root_disk_inode| {
            // 只能根目录节点使用
            assert!(root_disk_inode.is_dir());
            if let Some(old_inode_id) = self.find_inode_id(old_name, root_disk_inode) {
                let file_count = (root_disk_inode.size as usize) / DIRENT_SZ;
                let new_size = (file_count + 1) * DIRENT_SZ;
                // increase size
                self.increase_size(new_size as u32, root_disk_inode, &mut fs);
                // write dirent
                let dirent = DirEntry::new(new_name, old_inode_id);
                root_disk_inode.write_at(
                    file_count * DIRENT_SZ,
                    dirent.as_bytes(),
                    &self.block_device,
                );
                0
            } else {
                -1
            }
        })
    }

    /// 移除硬连接
    pub fn remove_link(&self, name: &str) -> isize {
        let fs = self.fs.lock();
        let mut deleted = false;
        self.modify_disk_inode(|root_disk_inode| {
            // 只能根目录节点使用
            assert!(root_disk_inode.is_dir());
            let file_count = (root_disk_inode.size as usize) / DIRENT_SZ;
            let mut dirent = DirEntry::empty();
            // let mut inode_id = 0;
            for i in 0..file_count {
                assert_eq!(
                    root_disk_inode.read_at(
                        DIRENT_SZ * i,
                        dirent.as_bytes_mut(),
                        &self.block_device,
                    ),
                    DIRENT_SZ,
                );
                let empty = DirEntry::empty();
                if dirent.name() == name {
                    root_disk_inode.write_at(i * DIRENT_SZ, empty.as_bytes(), &self.block_device);
                    // inode_id = dirent.inode_id();
                    deleted = true;
                }
            }
            // 在根目录里遍历寻找对应inode_id，如果没找到则释放对应的inode
            // for i in 0..file_count {
            //     assert_eq!(
            //         root_disk_inode.read_at(
            //             DIRENT_SZ * i,
            //             dirent.as_bytes_mut(),
            //             &self.block_device,
            //         ),
            //         DIRENT_SZ,
            //     );
            //     if dirent.inode_id() == inode_id {
            //         let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);
            //         let inode = Arc::new(Self::new(
            //             block_id,
            //             block_offset,
            //             self.fs.clone(),
            //             self.block_device.clone(),
            //         ));
            //         inode.clear();
            //     }
            // }
            if deleted {
                0
            } else {
                -1
            }
        })
    }
    /// Create inode under current inode by name
    pub fn create(&self, name: &str) -> Option<Arc<Inode>> {
        log::debug!("into create");

        let mut fs = self.fs.lock();
        let op = |root_inode: &DiskInode| {
            // assert it is a directory
            assert!(root_inode.is_dir());
            // has the file been created?
            self.find_inode_id(name, root_inode)
        };
        if self.read_disk_inode(op).is_some() {
            return None;
        }
        // create a new file
        // alloc a inode with an indirect block
        let new_inode_id = fs.alloc_inode();
        log::debug!("alloc inode id is {}", new_inode_id);
        // initialize inode
        let (new_inode_block_id, new_inode_block_offset) = fs.get_disk_inode_pos(new_inode_id);
        get_block_cache(new_inode_block_id as usize, Arc::clone(&self.block_device))
            .lock()
            .modify(new_inode_block_offset, |new_inode: &mut DiskInode| {
                new_inode.initialize(DiskInodeType::File);
            });
        self.modify_disk_inode(|root_inode| {
            // append file in the dirent
            let file_count = (root_inode.size as usize) / DIRENT_SZ;
            let new_size = (file_count + 1) * DIRENT_SZ;
            // increase size
            self.increase_size(new_size as u32, root_inode, &mut fs);
            // write dirent
            let dirent = DirEntry::new(name, new_inode_id);
            root_inode.write_at(
                file_count * DIRENT_SZ,
                dirent.as_bytes(),
                &self.block_device,
            );
        });

        let (block_id, block_offset) = fs.get_disk_inode_pos(new_inode_id);
        block_cache_sync_all();
        log::debug!("block id is {}, offset is {}", block_id, block_offset);
        // return inode
        let new_inode = Self::new(
            block_id,
            block_offset,
            self.fs.clone(),
            self.block_device.clone(),
        );
        {
            let mut inode_id = new_inode.inode_id.lock();
            *inode_id = new_inode_id as u64;
        }
        log::debug!("modifyed inode id is {}", *new_inode.inode_id.lock());
        Some(Arc::new(new_inode))
        // release efs lock automatically by compiler
    }
    /// List inodes under current inode
    pub fn ls(&self) -> Vec<String> {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            let file_count = (disk_inode.size as usize) / DIRENT_SZ;
            let mut v: Vec<String> = Vec::new();
            for i in 0..file_count {
                let mut dirent = DirEntry::empty();
                assert_eq!(
                    disk_inode.read_at(i * DIRENT_SZ, dirent.as_bytes_mut(), &self.block_device,),
                    DIRENT_SZ,
                );
                v.push(String::from(dirent.name()));
            }
            v
        })
    }
    /// Read data from current inode
    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| disk_inode.read_at(offset, buf, &self.block_device))
    }
    /// Write data to current inode
    pub fn write_at(&self, offset: usize, buf: &[u8]) -> usize {
        let mut fs = self.fs.lock();
        let size = self.modify_disk_inode(|disk_inode| {
            self.increase_size((offset + buf.len()) as u32, disk_inode, &mut fs);
            disk_inode.write_at(offset, buf, &self.block_device)
        });
        block_cache_sync_all();
        size
    }
    /// Clear the data in current inode
    pub fn clear(&self) {
        let mut fs = self.fs.lock();
        self.modify_disk_inode(|disk_inode| {
            let size = disk_inode.size;
            let data_blocks_dealloc = disk_inode.clear_size(&self.block_device);
            assert!(data_blocks_dealloc.len() == DiskInode::total_blocks(size) as usize);
            for data_block in data_blocks_dealloc.into_iter() {
                fs.dealloc_data(data_block);
            }
        });
        block_cache_sync_all();
    }
}
