#!/bin/zsh

set -e

cd efi;    cargo build -r --target aarch64-unknown-uefi; cd ..
cd kernel; cargo build -r --target aarch64-unknown-none; cd ..

mkdir -p dist/efi/boot
cp target/aarch64-unknown-uefi/release/unix-v11-efi.efi dist/efi/boot/bootaa64.efi
cp target/aarch64-unknown-none/release/unix-v11-kernel dist/unix-v11

dd if=/dev/zero of=unixv11.disk bs=1m count=64
diskno=$(hdiutil attach -imagekey diskimage-class=CRawDiskImage -nomount unixv11.disk | head -n 1 | awk '{print $1}')
diskutil eraseDisk FAT32 UNIXV11 GPTFormat $diskno
cp -R dist/* /Volumes/UNIXV11/
hdiutil detach $diskno

qemu-system-aarch64 -cpu cortex-a72 -machine virt,accel=tcg -smp 1 -bios OVMF-AArch64.fd \
 -drive file=unixv11.disk,if=none,id=drv0,format=raw -device nvme,drive=drv0,serial=unixv11nvme \
 -m 512M -serial stdio