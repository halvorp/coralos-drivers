// SPDX-License-Identifier: GPL-2.0-only
//! The standalone ELF's entry: `_start` hands straight to the library. CoralOS's own build links the same library from a two-line wrapper of
//! its own (so the closed tree never holds this source); this file is for building the device here.
#![no_std]
#![no_main]

/// The device's heap: demand-zero reservations (coralos-rt `heap.rs`), nothing in `.bss`. Growable tables use fallible growth, so
/// exhaustion is a named refusal, not an abort.
#[global_allocator]
static HEAP: coralos_rt::heap::LazyHeap = coralos_rt::heap::LazyHeap::new();

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    avs_device::rt::on_panic()
}

#[no_mangle]
pub extern "C" fn _start(role: u64) -> ! {
    match role {
        avs_device::rt::ROLE_SAFE_ONLY => avs_device::rt::safe_only(),
        _ => avs_device::rt::service(),
    }
}
