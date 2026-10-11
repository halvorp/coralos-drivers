// SPDX-License-Identifier: GPL-2.0-only
//! The standalone ELF's entry: `_start` hands straight to the library. CoralOS's own build links the same library from a
//! two-line wrapper of its own (so the closed tree never holds this source); this file is for building the device here.
#![no_std]
#![no_main]

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    i8042_device::on_panic()
}

#[no_mangle]
pub extern "C" fn _start(_role: u64) -> ! {
    i8042_device::service()
}
