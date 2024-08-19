use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use device_tree::{DeviceTree, Node};
use device_tree::util::SliceRead;
use lazy_static::lazy_static;
use log::{error, warn};
use lwext4_rust::bindings::ext4_direntry;
use lwext4_rust::InodeTypes;
use spin::lazy::Lazy;
use spin::RwLock;
use virtio_drivers_fs::device::blk::VirtIOBlk;
use virtio_drivers_fs::transport::{DeviceType, Transport};
use virtio_drivers_fs::transport::mmio::{MmioTransport, VirtIOHeader};
use crate::ext4fs_interface::disk::Disk;
use crate::ext4fs_interface::ext4fs::{Ext4FileSystem, FileWrapper, OpenFlags};
use crate::ext4fs_interface::ext4fs::OpenFlags::O_RDONLY;
use crate::ext4fs_interface::vfs_ops::{VfsNodeOps, VfsOps};
use crate::ext4fs_interface::virtio_impls::HalImpl;
use crate::sync::UPSafeCell;
use crate::task::current_user_token;

mod disk;
mod ext4fs;
mod vfs_ops;
mod virtio_impls;
pub fn init_dt(dtb: usize) -> Result<(), &'static str> {
    println!("device tree @ {:#x}", dtb);
    #[repr(C)]
    struct DtbHeader {
        be_magic: u32,
        be_size: u32,
    }
    let header = unsafe { &*(dtb as *const DtbHeader) };
    let magic = u32::from_be(header.be_magic);
    const DEVICE_TREE_MAGIC: u32 = 0xd00dfeed;
    assert_eq!(magic, DEVICE_TREE_MAGIC);
    let size = u32::from_be(header.be_size);
    let dtb_data = unsafe { core::slice::from_raw_parts(dtb as *const u8, size as usize) };
    let dt = DeviceTree::load(dtb_data).expect("failed to parse device tree");
    match walk_dt_node(&dt.root) {
        Ok(root) => {
            *ROOT_INODE.write() = Some(root);
            Ok(())
        }
        Err(e) => Err(e),
    }
}
fn walk_dt_node(dt: &Node) -> Result<Arc<dyn VfsNodeOps>, &'static str> {
    if let Ok(compatible) = dt.prop_str("compatible") {
        if compatible == "pci-host-ecam-generic" || compatible == "sifive,fu740-pcie" {
            if let Some(reg) = dt.prop_raw("reg") {
                let paddr = reg.as_slice().read_be_u64(0).unwrap_or(0);
                let size = reg
                    .as_slice()
                    .read_be_u64(2 * core::mem::size_of::<u32>())
                    .unwrap_or(0);

                let address_cells = dt.prop_u32("#address-cells").unwrap_or(0) as usize;
                let size_cells = dt.prop_u32("#size-cells").unwrap_or(0) as usize;
                let ranges = dt.prop_cells("ranges").unwrap();
                println!(
                    "pci ranges: bus_addr@[{:x?}], cpu_paddr@[{:x?}], size@[{:x?}]",
                    ranges[0]..ranges[address_cells - 1],
                    ranges[address_cells]..ranges[address_cells + 2 - 1],
                    ranges[address_cells + 2]..ranges[address_cells + 2 + size_cells - 1]
                );
                println!("{:?} addr={:#x}, size={:#x}", compatible, paddr, size);
            }
        } else if compatible == "virtio,mmio" {
            if let Some(reg) = dt.prop_raw("reg") {
                let paddr = reg.as_slice().read_be_u64(0).unwrap_or(0);
                let size = reg
                    .as_slice()
                    .read_be_u64(2 * core::mem::size_of::<u32>())
                    .unwrap_or(0);
                return virtio_probe(paddr, size);
            }
        }
    }

    for child in dt.children.iter() {
        if let Ok(result) = walk_dt_node(child) {
            return Ok(result);
        }
    }

    Err("Compatible device not found")
}

fn virtio_probe(paddr: u64, size: u64)->Result<Arc<dyn VfsNodeOps>,&'static str> {
    println!("walk dt addr={:#x}, size={:#x}", paddr, size);
    let vaddr = paddr;
    let header = core::ptr::NonNull::new(vaddr as *mut VirtIOHeader).unwrap();
    match unsafe { MmioTransport::new(header) } {
        Err(e) => {
            warn!("Construct a new VirtIO MMIO transport: {}", e);
            Err("Construct a new VirtIO MMIO transport")
        }
        Ok(transport) => {
            println!(
                "Detected virtio MMIO device with vendor id {:#X}, device type {:?}, version {:?}",
                transport.vendor_id(),
                transport.device_type(),
                transport.version(),
            );
            virtio_device(transport)
        }
    }
}
fn virtio_device(transport: impl Transport)->Result<Arc<dyn VfsNodeOps>,&'static str>{
    match transport.device_type() {
        DeviceType::Block => Ok(virtio_blk(transport)),
        t => {
            error!("Unrecognized virtio device: {:?}", t);
            Err("Unrecognized virtio device")
        },
    }
}
fn virtio_blk<T: Transport>(transport: T) -> Arc<dyn VfsNodeOps> {
    let mut blk = VirtIOBlk::<HalImpl, T>::new(transport).expect("failed to create blk driver");
    println!("VirtIOBlk driver created successfully");

    let root = init_ext4_fs(blk);
    println!("Ext4 file system initialized");

    *ROOT_INODE.write() = Some(root.clone());
    println!("ROOT_INODE set successfully");

    println!("virtio-blk test finished");
    root.clone()

}
pub fn init_ext4_fs<T: Transport>(dev: VirtIOBlk<HalImpl, T>) -> Arc<dyn VfsNodeOps> {
    let disk = Disk::new(dev);
    let ext4_fs: Box<dyn VfsOps> = Box::new(Ext4FileSystem::new(disk));
    let root = ext4_fs.root_dir();
    println!("Root directory initialized.");
    root
}

lazy_static! {
    pub static ref ROOT_INODE: RwLock<Option<Arc<dyn VfsNodeOps>>> = RwLock::new(None);
}
pub fn ext4_fs_test(){
    let file_path = "/sample.txt";
    let root_inode = ROOT_INODE.read().as_ref().expect("Root inode not initialized").clone();
    // 使用 root 来查找并打开文件
    let file_node = root_inode.lookup(file_path).unwrap();
    file_node.open_file(file_path, O_RDONLY);
    let file_size = file_node.get_file_size();
    println!("file_size={}",file_size);
    // 定义块大小
    const CHUNK_SIZE: usize = 4096; // 4KB
    // 用于存储读取的总字节数
    let mut total_read = 0;
    // 总缓冲区
    let mut total_buffer = Vec::with_capacity(file_size as usize);
    // 用于存储前几个字节（用于验证）
    let mut first_bytes = Vec::new();

    // 分块读取
    while total_read < file_size as usize {
        // 计算这次应该读取的字节数
        let bytes_to_read = CHUNK_SIZE.min(file_size as usize - total_read);

        // 创建一个缓冲区来存储这次读取的数据
        let mut buffer = vec![0u8; bytes_to_read];

        // 读取数据
        match file_node.read_at(total_read as u64, &mut buffer) {
            Ok(bytes_read) => {
                // 更新已读取的总字节数
                total_read += bytes_read;
                // 将读取的数据追加到总缓冲区
                total_buffer.extend_from_slice(&buffer[..bytes_read]);
                // 如果这是第一块，保存前几个字节用于验证
                if first_bytes.is_empty() && bytes_read > 0 {
                    first_bytes = buffer[..bytes_read.min(16)].to_vec(); // 保存最多16个字节
                }

                // 这里可以处理读取的数据
                println!("{} bytes were read, out of a total of {} bytes read", bytes_read, total_read);
                // 如果读取的字节数小于请求的字节数，说明已经到达文件末尾
                if bytes_read < bytes_to_read {
                    break;
                }
            },
            Err(e) => {
                println!("An error occurred while reading the file: {:?}", e);
                break;
            }
        }
    }

    println!("The file is read completely, with a total of {} bytes read", total_read);
    println!("The  {} bytes Of the file: {:?}", first_bytes.len(), first_bytes);
    // 打印所有读取的数据
    println!("The content of the file:");
    for (i, byte) in total_buffer.iter().enumerate() {
        print!("{:02x} ", byte);
        if (i + 1) % 16 == 0 {
            println!("");
        }
    }
    if total_buffer.len() % 16 != 0 {
        println!("");
    }

}