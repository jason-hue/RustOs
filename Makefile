DOCKER_TAG ?= rcore-tutorial-v3:latest
.PHONY: docker build_docker
	
docker:
	docker run --rm -it -v ${PWD}:/mnt -w /mnt --name rcore-tutorial-v3 ${DOCKER_TAG} bash

build_docker: 
	docker build -t ${DOCKER_TAG} --target build .

fmt:
	cd easy-fs; cargo fmt; cd ../easy-fs-fuse cargo fmt; cd ../os ; cargo fmt; cd ../user; cargo fmt; cd ..

all:
	@export PATH=$$PATH:/opt/riscv64-linux-musl-cross/bin; \
    cd os && mv cargo .cargo && make build; \
    cd .. && mv os/target/riscv64gc-unknown-none-elf/release/os ./kernel-qemu; \
    cp bootloader/rustsbi-qemu.bin sbi-qemu



run:
	cp sdcard-riscv.img/sdcard.img fat.img
	qemu-system-riscv64 -machine virt -kernel kernel-qemu -m 128M -nographic -smp 2 -bios sbi-qemu -drive file=fat.img,if=none,format=raw,id=x0  -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0 -device virtio-net-device,netdev=net -netdev user,id=net

clean:
	cd os && mv .cargo cargo && make clean
	cd fat32-fuse && cargo clean
	rm -rf ext4.img
	rm -rf kernel-qemu
	rm -rf sbi-qemu
