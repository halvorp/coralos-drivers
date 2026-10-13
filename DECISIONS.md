# Decisions

The owner's rulings that govern this repository. Newest first. Each is recorded as given; where a ruling is
still being worked out, it says so and names what is pending rather than guessing.

## 2026-10-12 — Architect (Astra), Kaby Lake GPU ruling, step 2

**D8 — `i915.device` boundary, read-only first.** The Intel GPU driver is ONE public `.device` that runs Kaby Lake by
runtime dispatch (Gen8 / Cherry Trail rows migrate into it later); nothing in it may import a helper from the closed tree's
`coral-i915-gem` or link its `coral-drm-driver` glue. Step 2 is the skeleton: `devices/i915` reads the PCI identity from the
function's configuration page, refuses any id outside `crates/i915-kbl-display-core`'s Kaby Lake table BY NAME (8086:591c, the
Pixelbook Go's, is in it; a Cherry Trail id is not and is never aliased to it), opens BAR0 and logs the display state the firmware
left (`[I915-KBL-READOUT pipe=…]`, one line per pipe, plus power wells, CDCLK, DDIs, DPLLs, planes, scalers), then answers every
wire request with an explicit UNSUPPORTED. It writes nothing. `rt/` gains `mmio.rs`: `ReadWindow`, a READ-ONLY, grant-bounded
view of a device window (no new system call or number: `MMIO_MAP` / `MMIO_LEN` were already listed). *Exposing an operation is not
granting it* - a read-only bring-up is not one typo from a write.

**PENDING architect review, named so they are not mistaken for rulings:** (1) the slot convention - BAR0 in MMIO slot 0 and the
config page, read-only, in slot 1 - is taken from how the closed tree endows its other PCI `.device`s; (2) the i915 wire
protocol's endpoint numbers (`SERVICE_EP` 13 / `REPLY_EP` 14, `devices/i915/src/wire.rs`) are PLACEHOLDERS until the loader's
endpoint table assigns them; (3) the readout's agreement with silicon is unproven until the Linux reconnaissance boot described
in `i915-kbl-display-core`'s crate docs has been compared against it.

## 2026-10-11 — Architect (Astra), keyboard ruling

**D7 — rt extended with capability-scoped port I/O and the physical key-event frame.** *Exposing an operation is not
granting it.* `rt/` gains `PORT_IO` (system call 52: ops IN8/OUT8/IN16/OUT16/IN32/OUT32 = 0..5, QUERY = 6, refusal
`u64::MAX`, rights IN/OUT/W8/W16/W32 = 1/2/4/8/16, 4 slots) behind an opaque `PortRange` handle, and the key frame
`[0x90, HID usage, state]` on endpoint 12 (state RELEASE 0 / PRESS 1 / REPEAT 2 / ALL_UP 3). A driver never names a raw port;
the loader grants `i8042.device` byte access to 0x60 and 0x64 as two ranges, plus IRQ 1. `devices/i8042` is the first
public GPL `.device`: it links only rt, hal and the three cores (i8042-core, atkbd-scancode-core, hid-input-keyboard-core).
It emits PHYSICAL key events; layout, modifiers, CapsLock, text and the ChromeOS top-row policy stay in the input layer.

## 2026-10-11 — Peter (owner)

**D1 — Linux drivers are ported, not reinvented.** In the owner's words: *"we have the open source repo for the GPL
derived code, so you can take all the hardware drivers from Linux and rewrite them in Rust, adjusted to work with
CoralOS."* A port is mechanical: every register offset, mask, sequence and message is taken from the Linux source
and cited by file and line (PORT_CONVENTIONS.md §1). BSD and Haiku drivers are second opinions, read when the Linux
source is ambiguous; they are not the port source.

**D2 — This repository is the only home of GPL-derived CoralOS code.** One standalone `no_std` crate per port,
`crates/<name>-core`, with its own `[workspace]`, `license` naming the Linux file's licence (normally
`GPL-2.0-only`) and an SPDX line on every file (PORT_CONVENTIONS.md §6, §7a). The CoralOS operating system tree is
closed source and does not contain GPL-derived code; code written there from public specifications may cite Linux
for facts, but ported logic belongs here.

**D3 — No broad HAL.** `hal/` (`coralos-hal`, MIT OR Apache-2.0) is the `Mmio` trait plus the cooperative-wait
hooks, mirrored verbatim from CoralOS's own seam. It grows only when a real driver needs something, and only by
what that driver needs. The next additions are `Dma` and `Irq` traits, for the Intel Wireless 9260 (iwlwifi) port.
Ports stay pure: values in, values out, no MMIO of their own (PORT_CONVENTIONS.md §2).

**D4 — Licence shape of a driver binary.** A `.device` driver binary that links a GPL port is GPL. It links only
GPL-compatible code: the port crates, `coralos-hal`, and a minimal permissive driver runtime (the system-call shim
for MMIO, DMA, IRQ, IPC and logging) that will live in this repository. The `.device` driver crates themselves live
here too. CoralOS uses a driver only as a separate process, across its process / capability boundary — never by
linking it into its own binaries.

The runtime is `rt/` (`coralos-rt`, MIT OR Apache-2.0), published with the owner's approval on 2026-10-11 ("yes,
publish rt/ to the drivers repo"). Its header lists every CoralOS ABI fact it makes public: the `syscall` calling
convention, sixteen system-call numbers, the return words, the DMA out-record and cache argument, and the `_start(role)`
entry. Anything a driver needs beyond that list is added here deliberately, with the same review — never by linking a
proprietary CoralOS crate. Still to come: logging/formatting and per-device slot
numbers, the network and Wi-Fi frame formats, and a kernel call that frees DMA memory.

**D5 — Consumption is pinned.** CoralOS builds consume this repository at a pinned revision, and a check in the
CoralOS build verifies the pin: a driver crate names this repository as a cargo git dependency with an exact `rev`,
pinned once in CoralOS's `tools/drivers.lock`, and CoralOS's `tools/check-drivers-pin.sh` fails the build on a branch,
another rev, a local path, a stale Cargo.lock, or a rev not on this repository's `main`.

**D6 — Primary evidence over summaries.** A hardware fact is taken from primary evidence — the board's firmware or
recovery image, coreboot, the Linux source, a measurement on the machine — never from an external summary of it.
On 2026-10-11 a web summary of the Pixelbook Go (ATLAS) listed an Intel AC 7265, NVMe storage and a Synaptics
touchpad; all three were wrong against the board's own recovery image. A port's choice of which Linux driver to
port is grounded the same way.

**D9 — A `.device` heap is a chain of demand-zero regions with a reusing free list (2026-10-12).** `rt/src/heap.rs` (`LazyHeap`,
MIT OR Apache-2.0) is the `#[global_allocator]` of a `.device`. Mechanism: each REGION is one anonymous `MMAP` reservation
(HEAP-IS-DEMAND-ZERO - address space only, the kernel commits a zeroed frame per page on first touch; never a zeroed `.bss`
arena), the first made on the first allocation; when no region can serve a request ANOTHER is reserved (one 16 MiB chunk, or more
for a large request), so there is no compile-time total (RUNTIME-TABLES-HAVE-NO-CAP) - the heap is out of memory only when the
kernel refuses a reservation, and then `alloc` returns null and `Vec::try_reserve` and friends give their named refusals before
the thing exists. Policy: `linked_list_allocator` 0.10 (MIT OR Apache-2.0, GPL-compatible; chosen by the YAGNI ladder because it
is already the CoralOS tree's allocator and in the local registry) - first-fit free list with coalescing inside each region, so
freed memory IS reused; each region's header lives in the region. Regions are never unmapped (the ABI has no unmap). No lock: a
`.device` is one task with one thread of control (the ABI has no thread call and delivers IRQs as `IRQ_WAIT` returns), and `Sync`
is asserted on exactly that; a host harness that runs threads serialises with its own mutex. Tests (`rt/tests/heap_global.rs`):
1,000,000 mixed-size, mixed-alignment alloc/free ops reserve at most a small multiple of the peak live set; growth past the first
region and a request larger than a chunk; null on a refused first or later reservation with the heap still working; a growable
`Vec` under it as the global allocator. The CoralOS-side wrapper of `i915.device` must declare the same `#[global_allocator]` (as
`devices/i915/src/main.rs` does). The mechanism is the ABI's `MMAP` only (no new call); the policy was written from the design
idea, not copied from the proprietary tree.

**D10 — Core crates may use `alloc` for runtime-growing tables; fallible growth only; named refusal.** Owner ruling 2026-10-12: "if we are
a real OS, there cannot be a hard cap" (doctrine RUNTIME-TABLES-HAVE-NO-CAP). A table that tracks a runtime-growing population
(sessions, handles, objects, contexts, address-space entries, requests, leases) is an `alloc` collection that grows with memory; its
size is never a compile-time constant. Growth is fallible (`try_reserve` / a charged `try_insert`; no `Vec::push` on an unreserved vector,
no `collect`, no `vec!` on a request path); exhaustion is a NAMED refusal (`GEM-NOMEM`, naming the table) raised before the thing
exists, with nothing half-created. This is the justification PORT_CONVENTIONS section 7 asks for ("No `alloc` unless unavoidable and
justified in a comment"); the comment sits at the top of the crate's `lib.rs`. What stays fixed is fixed by HARDWARE (engines on the part,
context tags, fence registers) or by an ABI width, and says so where it is declared.

**D11 — Atlas audio wave 2: the codec, the controller and the boot are PLANS; the speakers are unreachable by construction (2026-10-13).**
`da7219-core` builds only transactions to the codec of a `Wiring` (the board's I2C addresses, handed in from the board row - none lives in a core crate); the single place a wire message is made refuses the speaker amplifiers on the same bus by name (Atlas: MAX98373 0x31/0x32), `avs-boot-core` refuses a back end on any of the board's speaker SSPs before a pipeline exists, and tests run every plan builder through a fake codec asserting it. The codec's SAFE STATE (outputs driven
off, amplifier then DAC muted, stages powered down) is proven by register readback, and `avs-boot-core` asks for that proof (`Action::AskCodecSafe`)
BEFORE the host DMA starts or any pipeline leaves RESET: the I2S clocks cannot start on a codec of unknown state. DAPM power-up leaves every
mute bit set (a checked precondition and postcondition) and the jack output enable is the accessory detect's, as in Linux; unmute is a separate
plan, DAC before amplifier. The headphone-impedance test (a 0 dB tone through the jack) is NOT ported; a 3-pole jack is reported as headphones
and says so. A peakvol module's `target_volume` in the topology is its ceiling: the module is initialised SILENT (Linux: unity) and a request above
the ceiling is refused by name; a path without peakvol (da7219) reports none and the codec gain ceiling (`hp::GainPolicy`) is the limit.
`avs-fw-core::effect::Width` gained `W16`: a 16-bit register is never written through a 32-bit access. Plans are fixed-capacity sequences
(`PLAN_CAP`, `PROGRAM_CAP`: a power sequence this code writes down, not a table a client drives); everything a file or the hardware sizes
(multi-link list, BDL entries, the message script, IPC payload copies) is a fallibly-grown `Vec` or a hardware-defined count with its reason (D10).

**D12 — A `.device` serves clients over `connect()` sessions; a frame travels as messages that end where its header says (2026-10-13).**
Two gaps on the path to iris, found by the CoralOS shim worker. (1) PER-CLIENT SESSIONS: a static request/reply endpoint pair cannot route a
reply to the process that asked. `devices/i915` now RECVs connect announces on a LISTEN endpoint (`EP_I915_REQ` 65); a client's `connect()` mints
it a private (request, event) pair from the kernel's dynamic pool and the device serves it as one connection (`src/conn`): at most one uAPI
session per connection, naming another connection's incarnation is refused by name (`SESSION-NOT-ON-THIS-CONNECTION`), PEER_DEAD on a
connection's request endpoint (or a send that finds it dead) is `session_dead` for that session only, and the pair is given back with
SESSION_RELEASE. `coralos-rt` gains the minimal wrappers for SERVING a session, no new authority: `session_release(req, evt)` (system call 100,
capability-checked by the kernel against the caller's recorded serving set) and `rt::session` (the announce `[0xC0, req, evt, client]` and tombstone
`[0xDD, client]` layouts, restated from the kernel's `session_wire`, and the 128-endpoint wait-set words, which refuse an endpoint a shift would wrap
onto another). `connect` was already there. The connection table grows with memory and a row is reserved BEFORE a connect is accepted: with no room
the loop stops listening, so the kernel's listen ring fills and the kernel refuses the connect - a named refusal before a session exists
(RUNTIME-TABLES-HAVE-NO-CAP, D10). The wait set is the listen endpoint plus every connection's request endpoint (kernel width: 128 endpoints).
The kernel wakes the LOWEST ready endpoint first, so a client that keeps its ring full can delay higher-numbered ones; not addressed here.
(2) FRAME REASSEMBLY: the kernel's message is 4096 bytes (`send` truncates), a ring holds 16 (a full ring drops the newest). `i915_wire::frag` is
the ONE definition (MIT OR Apache-2.0, so the closed shim compiles the same code): a frame is cut into consecutive full messages, the last holding
the remainder, and ends where its own header says; at most 15 messages = 61440 bytes (a larger call travels as a payload region). `split` is the
sender, `Joiner` the receiver (three counters, no bytes, so each side keeps the frame in its own buffer: the device reserves exactly the header's
declared length per connection, after the wire has bounded it). Refusals are named: `FRAG-FIRST-SHORT`, `FRAG-TOO-MANY-MESSAGES`,
`FRAG-SHORT-FRAGMENT`, `FRAG-INTERLEAVED`, `FRAG-OVERLONG`, plus the wire's own header names; the request a half-received frame belonged to is
answered. The kernel's tombstone is told from data only BETWEEN frames. The device's parked `service()` uses the same connections and the same
reassembly (`conn::Parked`). Retired: the static reply endpoint 66 (`EP_I915_REPLY`) and the parked loop's 10 ms pace.

**D13 — A `.device` reads a client's payload and gives it CPU access to device pages only through the kernel's session-scoped region calls (2026-10-13).**
Astra's ruling (4/4 with gemini, deepseek, kimi) on the Kaby Lake GPU path; the kernel's REGION_MAP (113) / REGION_UNMAP (114) / DMA_LEASE_CREATE (115)
/ DMA_LEASE_REVOKE (116). `coralos-rt` gains `rt::region` (MIT OR Apache-2.0): the wrappers with typed status enums (`RegionStatus` MAX-130..=MAX-140 (incl. `WrongSession`),
`LeaseStatus` MAX-141..=MAX-151, `LeaseRequest` 64 bytes with `out_lease` at 56), restated from the kernel and pinned by `rt/tests/region.rs` (as D12 did for
`session_release`), plus `read_region`/`write_region` - map, copy ONCE, unmap - the only place a pointer into a client's memory exists. GAP, documented in
`region.rs`: rt has no `SIGACTION` wrapper, restorer or resumable copy, so a mapping revoked WHILE that copy runs faults the device instead of becoming
`PAYLOAD-REGION-REVOKED`; the exposure is one memcpy per region call. (1) PAYLOAD: `live::SessionRegions` is the `PayloadRegions` binding. The client delegates request storage READ-ONLY and reply storage READ-WRITE over its
session as SESSION CAPABILITIES (CAP_DELEGATE with `DELEGATE_SESSION_CAP`: `DELEG_CAP_TAG | id` in a growable kernel ledger, never reused, no shmem slot) and
sends the ids (`RegionRef::region`, `reply_region` with `REPLY_ELSEWHERE`, ABI 1.3; the device ORs the tag back in, so only session capabilities are
nameable). Every REGION_MAP names the session: the request endpoint the call ARRIVED on (the transport's fact, never the request bytes) in bits 16..31 of
the rights argument; a capability that arrived on another session is `REGION-WRONG-SESSION` (`PAYLOAD-REGION-WRONG-SESSION`, EACCES), so the confused
deputy and the 24-slot cap of the first cut are closed in the kernel and the device keeps no ownership table. The device snapshots the request into its own
buffer BEFORE validating (a read-only delegation does not stop the client changing its pages); nothing stays mapped between calls, so a session that ended is a
named kernel refusal, not a stale pointer. rt also wraps `region_cap_drop` (REGION_UNMAP of a capability); the device does not call it - capabilities die with
their session, and a client that wants to retire one early does so itself. The 61 KiB inline ceiling is lifted; the 4 MiB wire bound stays a validation limit. (2) MMAP:
`GEM-MMAP-OPEN` answers with a kernel lease (`uapi::mmap_lease`, ABI 1.3: `region_cap`, `region_offset`), one per object over its whole page run, recipient =
the connection's request endpoint + client node from the connect announce (`conn::Peer`), cache = the pages' own (`exec::dma::BO_COHERENCE`, non-snooped, so WC
- the mmap reply says what the lease IS). Revoked when the object is freed (after retirement and unbinding) or the session ends, never by GEM_CLOSE alone; the
pages return to the pool only after the kernel confirmed the end, else they are QUARANTINED (counted, logged, never reused). The lease table grows (D10).
Linux-derived semantics (mmap offsets, object lifetime) stay in gem-core; the kernel mechanism is never asked to interpret GTT state.

**D14 — `avs.device`: the Atlas headphone path as ONE public `.device`; the DSP lives only while a stream plays; every fence is a list that a test proves equal to what the plans write (2026-10-13).**
`devices/avs` executes the wave-1/wave-2 plans (it adds no policy to them) and serves Aura.engine over `wire/avs-wire` (MIT OR Apache-2.0; its
`board` module is the board record the loader writes and the device refuses to start without). HEADPHONES ONLY: the speaker route is answered
`SPEAKER-PROTECTION-UNAVAILABLE` and touches nothing; amplifier addresses are refused at two layers (`da7219-core::Wiring` and `rig::I2cAllow`); the
speaker-reset pad (`SPKR_RST_L`) is driven low first of all and read back, a refusal to grant that write is the named gap `SPEAKER-RESET-PAD` and a
pad that reads high is `INIT-FAILED(speaker-hold)`. **Lifecycle:** the device attaches the codec and parses every file at start (the status can then say
FIRMWARE-UNAVAILABLE(file) / BACKEND-UNAVAILABLE(step) / INIT-FAILED(step) / HEADPHONE-DISCONNECTED before anyone asks), but boots the DSP only for a
running stream and powers it off (codec to the verified safe state first) when the stream stops, the jack is pulled, a fault occurs or the client dies.
The cost is a boot per start; the benefit is no idle power, no state that can go stale, and one path to verify. **Fences:** `allow.rs` lists the BAR4 /
BAR0 / configuration registers (offset AND width) the plans write, built once discovery has read the controller; the production machine builds its
window grants FROM that list (`Machine::layout_ready`) after opening every window read-only; `tests/fences.rs` runs a whole life of the device and
requires set equality with the writes the simulated hardware saw, and shows the two real fences (an extra entry, a removed address check) failing.
The I2C allow-list is the codec's address and no other; the pad allow-list is one `PADCFG0`. **Waiting:** idle is one blocking wait on requests and the
jack event with no deadline; a running stream adds the cAVS interrupt and a watchdog of three rings; plans' polls sleep between reads on the same
primitive; the codec's long sequences service the playing stream between steps. A level interrupt with no cause 64 times in a row is `irq-storm`; a codec
line that stays asserted after 8 passes is `jack-irq-storm` and is left masked at its owner until the client's next start. **Wire and kernel gaps, named:**
PCM travels inline (`PCM-WRITE`, 16 KiB) until the kernel can map a client region; the region form (`STREAM-OPEN-RING`) is defined and answered
`RING-REGION-UNAVAILABLE`. The codec's interrupt reaches the device as a message from the pad interrupt owner because one `WAIT_ANY_IRQ` waits on one line.
The client is served over a `connect()` session on ONE LISTEN endpoint (one session at a time; a second connect waits in the kernel's queue), and the dead
client's pair is returned with `session_release`. The five endpoint numbers in `devices/avs/src/wire.rs` (67..=71) are PROPOSED registry entries. `rt` gains `WriteWindow::write8` / `write16` (the stream
descriptors have byte and halfword registers; a narrow write inside an allowed dword changes only its own bytes), additive and under the same list. The
pipeline latency from host DMA to the pin is reported as UNMEASURED (`u32::MAX`), never guessed; the audio clock is the host stream's `LPIB`, not
submitted bytes. UNVERIFIED ON SILICON (named, not claimed): the SSP/codec clock order (the codec is powered up AFTER the pipelines run, because the SRM PLL
locks to the word clock), MSI vs INTx for the cAVS line, the I2C controller's fast-mode counts, the pad community window layout, `GPP_F10`'s interrupt route.

**D-NEXT — A buffer or a fence crosses processes only as a GRANT addressed to one NAMED session; a number is never the authority (2026-10-13).**
Astra's GPU-kernel ruling Q3 ("PRIME transfers a recipient-bound BO grant, redeemed into an independent handle/lease - not the exporter's revocable CPU lease. Fence
grants provide wait-only authority; device events drive blocking poll. Descriptor numbers remain process-local."), wire ABI 1.4 (additive). Before this the PRIME
export token was a bearer number: `PRIME-FD-TO-HANDLE` honoured it from ANY session, and the u64 was a counter. Now: (1) a token minted by `PRIME-HANDLE-TO-FD` is
addressed to its exporter only; (2) `PRIME-GRANT {handle, flags, recipient}` mints a token addressed to the session incarnation `recipient` - a live session of this
device (the incarnation is what the recipient's SESSION-OPEN reply told it; the exporter learns it from the recipient over whatever channel the two processes share) -
redeemable only by a `PRIME-FD-TO-HANDLE` that ARRIVES on that session's own endpoint (the kernel lets only that client send there), into the recipient's OWN handle;
a stolen, copied or unknown token is `EINVAL` either way (named `PRIME-NOT-RECIPIENT` / `PRIME-NO-SUCH-TOKEN` on the log only: no oracle for which tokens exist);
(3) `SYNC-FILE-GRANT {token, recipient}` / `SYNC-FILE-REDEEM {grant}`: the grant holds ONE reference on the fence behind the exporter's sync_file (so the exporter may close its
own at once), is spent by the redeem, and gives the recipient a sync_file token of its own session over the same fence - wait-only by construction (a sync_file can be
waited on, merged and used as a dependency; nothing signals through it); `FD-CLOSE` kind 3 withdraws an unredeemed grant. (4) Lifetime: a grant ends with its exporter OR
its recipient; a handle or sync_file already redeemed holds its own reference and is untouched. NOT in this row: syncobj export tokens (`SYNCOBJ-HANDLE-TO-FD`
without EXPORT_SYNC_FILE) are still the old bearer form - a full syncobj is signalable, so it needs its own ruling; the importer's CPU lease on a shared object is still
refused while the exporter maps it (`MMAP-OBJECT-LEASED-TO-ANOTHER-SESSION`) - each side getting its own lease needs the page-exclusive lease rule lifted in gem-core.
