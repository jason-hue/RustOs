use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use device_tree::{DeviceTree, Node};
use device_tree::util::SliceRead;
use log::{error, warn};
use lwext4_rust::bindings::ext4_direntry;
use lwext4_rust::InodeTypes;
use virtio_drivers_fs::device::blk::VirtIOBlk;
use virtio_drivers_fs::transport::{DeviceType, Transport};
use virtio_drivers_fs::transport::mmio::{MmioTransport, VirtIOHeader};
use crate::ext4fs_interface::disk::Disk;
use crate::ext4fs_interface::ext4fs::{Ext4FileSystem, FileWrapper, OpenFlags};
use crate::ext4fs_interface::ext4fs::OpenFlags::O_RDONLY;
use crate::ext4fs_interface::vfs_ops::{VfsNodeOps, VfsOps};
use crate::ext4fs_interface::virtio_impls::HalImpl;
use crate::mm::{translated_pa_to_va, translated_va_to_pa};
use crate::task::current_user_token;

mod disk;
mod ext4fs;
mod vfs_ops;
mod virtio_impls;
pub fn init_dt(dtb: usize) {
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
    walk_dt_node(&dt.root);
}

fn walk_dt_node(dt: &Node) {
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
                //pci_scan().unwrap();
            }
        }

        if compatible == "virtio,mmio" {
            if let Some(reg) = dt.prop_raw("reg") {
                let paddr = reg.as_slice().read_be_u64(0).unwrap_or(0);
                let size = reg
                    .as_slice()
                    .read_be_u64(2 * core::mem::size_of::<u32>())
                    .unwrap_or(0);
                virtio_probe(paddr, size);
            }
        }
    }
    for child in dt.children.iter() {
        walk_dt_node(child);
    }
}
fn virtio_probe(paddr: u64, size: u64) {
    println!("walk dt addr={:#x}, size={:#x}", paddr, size);
    let vaddr = paddr;
    let header = core::ptr::NonNull::new(vaddr as *mut VirtIOHeader).unwrap();
    match unsafe { MmioTransport::new(header) } {
        Err(e) => warn!("Construct a new VirtIO MMIO transport: {}", e),
        Ok(transport) => {
            println!(
                "Detected virtio MMIO device with vendor id {:#X}, device type {:?}, version {:?}",
                transport.vendor_id(),
                transport.device_type(),
                transport.version(),
            );
            virtio_device(transport);
        }
    }
}
fn virtio_device(transport: impl Transport) {
    match transport.device_type() {
        DeviceType::Block => virtio_blk(transport),
        DeviceType::GPU => println!("VirtIO GPU"),
        DeviceType::Input => println!("VirtIO Input"),
        DeviceType::Network => println!("VirtIO Network"),
        t => error!("Unrecognized virtio device: {:?}", t),
    }
}
fn virtio_blk<T: Transport>(transport: T) {
    let mut blk = VirtIOBlk::<HalImpl, T>::new(transport).expect("failed to create blk driver");
    init_rootfs(blk);
    println!("virtio-blk test finished");
}

pub fn init_rootfs<T: Transport>(dev: VirtIOBlk<HalImpl, T>) {
    let disk = Disk::new(dev);
    let ext4_fs: Box<dyn VfsOps> = Box::new(Ext4FileSystem::new(disk));
    let root = ext4_fs.root_dir();

    // let new_file = "/sample.text";
    // root.create(new_file, InodeTypes::EXT4_DE_REG_FILE);
    //
    // let mut new_fd: Box<dyn VfsNodeOps> =
    //     Box::new(FileWrapper::new(new_file, InodeTypes::EXT4_INODE_MODE_FILE));
    //
    // let mut write_buf: [u8; 20] = [0xFF; 20];
    // let mut read_buf: [u8; 20] = [0; 20];
    //
    // // new_fd.write_at(0, &write_buf);
    //
    // new_fd.read_at(0, &mut read_buf);
    //
    // // root.remove(new_file);
    //
    // println!("read file = {:#x?}", read_buf);
    // assert_eq!(write_buf, read_buf);

    let file_path = "/busybox";

    // 打开已经存在的文件
    let mut file_fd: Box<dyn VfsNodeOps> =
        Box::new(FileWrapper::new(file_path, InodeTypes::EXT4_INODE_MODE_FILE));
    let file_size = file_fd.get_file_size(file_path);
    println!("file_size={}",file_size);
    // let mut read_buf = vec![0; file_size as usize];
    // // 从文件中读取
    // file_fd.read_at(0, &mut read_buf).expect("Failed to read from file");
    //
    // // 输出读取的数据
    // println!("read file = {:#x?}", read_buf);
    // 定义块大小
    const CHUNK_SIZE: usize = 4096; // 4KB

    // 用于存储读取的总字节数
    let mut total_read = 0;

    // 用于存储前几个字节（用于验证）
    let mut first_bytes = Vec::new();

    // 分块读取
    while total_read < file_size as usize {
        // 计算这次应该读取的字节数
        let bytes_to_read = CHUNK_SIZE.min(file_size as usize - total_read);

        // 创建一个缓冲区来存储这次读取的数据
        let mut buffer = vec![0u8; bytes_to_read];

        // 读取数据
        match file_fd.read_at(total_read as u64, &mut buffer) {
            Ok(bytes_read) => {
                // 更新已读取的总字节数
                total_read += bytes_read;

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

    drop(ext4_fs);
}

