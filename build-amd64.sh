#!/bin/zsh

set -e

cd efi;    cargo build -r --target x86_64-unknown-uefi; cd ..
cd kernel; cargo build -r --target x86_64-unknown-none; cd ..

mkdir -p dist/efi/boot
cp target/x86_64-unknown-uefi/release/unix-v11-efi.efi dist/efi/boot/bootx64.efi
cp target/x86_64-unknown-none/release/unix-v11-kernel dist/unix-v11

dd if=/dev/zero of=unixv11.disk bs=1M count=64
diskno=$(hdiutil attach -imagekey diskimage-class=CRawDiskImage -nomount unixv11.disk | head -n 1 | awk '{print $1}')
diskutil eraseDisk FAT32 UNIXV11 GPTFormat $diskno
cp -R dist/* /Volumes/UNIXV11/
hdiutil detach $diskno

qemu-system-x86_64 -cpu Skylake-Client -machine q35 -smp 1 -bios OVMF-AMD64.fd \
 -drive file=unixv11.disk,if=none,id=drv0,format=raw -device nvme,drive=drv0,serial=unixv11nvme \
 -m 512M -serial stdio