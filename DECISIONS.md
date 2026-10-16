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
*(Amended by D17, 2026-10-16: the speaker clauses below - "unreachable by construction", "refused by name" - are replaced by the PROTECTED speaker path; the headphone and safe-state clauses stand.)*
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
*(Amended by D17, 2026-10-16: "HEADPHONES ONLY", the `SPEAKER-PROTECTION-UNAVAILABLE` answer to every speaker route and the amplifiers' unreachable I2C addresses become the protected speaker path; the lifecycle and the fences stand and grow.)*
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

**D16 — the avs.device endpoint numbers are the CoralOS registry's: 70..=74 (2026-10-16).**
The five numbers of `devices/avs/src/wire.rs` were proposed as 67..=71 (D14). The CoralOS registry gave the static endpoints below 70 to other services before the audio
integration landed (the touchpad-prefs batch, matrix.connector, messages.service's reader lane, and storage's vault compartment: `EP_FS_VAULT` = 69), so the device carries
`AVS_LISTEN` 70, `AVS_FW_REQ` 71, `AVS_FW_REPLY` 72, `AVS_JACK` 73, `AVS_JACK_ACK` 74 and the registry's dynamic base is 75. Nothing else changes: the device reads them only
in `rt`, and the wait mask already covers endpoints above 63. A later registry reshuffle renumbers this file again, in the same change that moves the loader's grants.

**D17 — the Pixelbook Go's speakers work, PROTECTED: SPEAKERS-ARE-REFUSED-UNTIL-PROTECTED is amended, not bypassed (2026-10-16).**
Owner ruling 2026-10-16 (the speakers played on the real unit under Alpine Linux 6.18 and were heard): the speakers must work. D11/D14's "refused until a speaker-protection path exists" stays a rule; what "protected" MEANS changes to what can be built and shown now, in four parts, all enforced by tests in this repository.
(1) **The amplifiers' own protections are ON with ChromeOS's values, and applied BEFORE anything is enabled or clocked.** `crates/max98373-core` is the MAX98373 as plans (Linux `max98373.c`, `max98373-i2c.c`, `avs/boards/max98373.c`, every register with file and line): `ctl::VERB` is the 68 `cset`s of `usr/share/alsa/ucm/avs_max98373/HiFi.conf` `SectionVerb.EnableSequence` (DAI select, output voltage, speaker/digital volume, DHT, BDE levels 1-4 with thresholds, clip reduction, clip threshold and limiter threshold, hold, hysteresis, attack/release) and `ctl::DEVICE` the ten of `SectionDevice."Speaker"` (output voltage 5 = 9.65 V, DHT, BDE and VI sense ON, the speaker pin ON), Left then Right for each control in the file's order, with the ALSA control arithmetic (`soc_mixer_control`, `soc_enum`) turning each number into register bits. `avs.device` (`devices/avs/src/speakers.rs`) performs them over I2C and then READS BACK every register it set; only if all hold does anything continue. The order of a speaker session: amplifiers held in hardware reset (the pad, D11's hold, now also at every way out) -> reset released after >= 50 ms low (+ 20 ms settle, `max98373_slot_config`) -> per part: soft reset, probe defaults, DAI format DSP_B / TDM 8 x 16 / 48 kHz and its TDM slots -> the UCM set -> readback -> DSP boot with the I2S clocks (the boot's `AskCodecSafe` gate asks that both parts are not enabled) -> `SPK_EN`, `PCM_RX_EN`, `GLOBAL_EN` + 30 ms settle (`Amp Enable`'s widget sequence, max98373.c:26-32) -> the DSP volume stage is raised from silence to the client's level. Two deliberate differences from Linux, both stricter: `SPK_EN` (probe, max98373.c:424) waits until the protection is proven, and the parts are enabled AFTER their clocks exist (Linux enables first and leans on the clock monitor). The UCM's DisableSequence (DHT/BDE OFF) is NOT run: a protection is never switched off while the part could be enabled; the part goes back into reset instead.
(2) **A hard digital gain ceiling inside `avs.device` that no client can exceed.** `max98373_core::gain`: the speaker path's DSP volume stage (peakvol, the topology's "DSP Volume") never receives a Q1.31 gain above `CEILING_LINEAR` = 539423503 (0x2026F30F) = -12.00 dB re full scale. Derivation (also in `gain.rs`): ChromeOS's `etc/cras/avs_max98373` is an explicit curve whose step 100 is 0 dB - full scale reaches the amplifiers at 9.65 V, which is only survivable because DSM watches the cones. The amplifier's DHT/BDE protect the part and the battery, not the cone. Full-scale 9.65 V peak is 5.8 W (8 ohm) / 11.6 W (4 ohm) per amplifier for a sine, a laptop speaker is rated roughly 1-2 W; the highest CRAS step at or below -12.00 dB is step 69 (`db_at_69 = -1200`), which peaks at 2.43 V = 0.37 W / 0.74 W and is 28 dB above the -40 dBFS tone the owner called clearly audible. The speaker's actual rating is NOT in the extract (the 8/4 ohm figures are the part's two supported loads): this is a judgment, recorded as one, to be replaced by a measurement. Aura's level 0..1000 maps to 64 steps of 0.75 dB below the ceiling (`GAIN_Q31`, recomputed from `10^(dB/20)` by a test); level 0 or mute is silence. Two checks stand between any request and the DSP: `session::set_speaker_gain` clamps to the ceiling and `avs-boot-core`'s `PeakvolHandle` (its ceiling = the number the device names, lowered never raised by anything the topology declares) refuses anything above it; a speaker path with no volume stage is refused by the planner (`SpeakerPathWithoutVolume`), and a speaker back end is planned only when the caller names the ceiling (`PlaybackSel::speaker_ceiling`), so a headphone request for the speaker SSP is still `SpeakerPathRefused`. The wire (`avs-wire`) already refuses a level past 1000 as malformed.
(3) **The topology is the upstream DSM-free one.** `max98373-tplg.bin` = `thesofproject/avs-topology-xml@221e4a6e652e15c54b21ae018c32ae0ab3e4eab1 i2s/max98373-tplg.xml` (Apache-2.0; sha256 1800b276993fb522cdc35ed8f11602ee19ac0e6ca2b0d71432175f97aea39c47) compiled with avsdk@5706230 (`avstplg`, net8.0 retarget) and `alsatplg` to 3748 bytes, sha256 ae9b39df33277b3c35693357af02d498e305fbc8da709af9c70552f9cb654c7f. ChromeOS's 7632-byte file of the same name carries the DSM control mainline cannot load (`no complete control IO handler for DSM data type`, ATLAS-PERIPHERALS-FROM-RECOVERY.md 3.6) and is NOT used. It reaches the device as a firmware file through the broker like every other (CoralOS `firmware/intel/avs/`, `WHENCE`); `Topology::parse` binds the speaker SSP (`ssp%d` -> `ssp0`), the planner takes its front end's closing gateway-less copier (`AVS_DMA_NO_GATEWAY`), a missing or corrupt file makes the speakers `FIRMWARE-UNAVAILABLE` naming it and leaves the headphones alone.
(4) **DSM stays a named later upgrade.** The two `cset-tlv` blobs of the UCM verb (`DSM data`, `WAVES data`) and `dsm_core` are not ported. *delete-when:* the DSM library is loaded and `DSM data` (`/opt/google/dsm/dsmparam.bin`, sha256 ab2f7753...) applied with the I/V feedback stream captured, shown working on silicon by the owner's measurements; then the ceiling is lifted to the curve's own maximum in the same change and this paragraph's (2) is rewritten.
**The fences grow, they do not vanish.** The I2C allow-list admits an amplifier address only while a speaker session has ARMED it, only for the registers `plan::written_registers` derives from the plans (44, plus a read of the revision id), and only as a one-register write or a one-register read; unarmed it refuses the address by name as before (`I2C-SPEAKER-AMPLIFIER-REFUSED`), and an armed register no plan writes (the tone generator, the thermal limits) is `I2C-AMPLIFIER-REGISTER-REFUSED`. The reset pad stays the one pad register the device may write. **Every way out is the safe state:** stop, headphones plugged (an `Auto` stream), a fault, the client's death and a dead task all end with the DSP volume at silence, `GLOBAL_EN` off and proven, the reset line driven low and proven by the pad's readback, and the fence disarmed; a dying task writes only `GLOBAL_EN` off and the pad (no allocation); the `safe-only` relaunch holds the line low without addressing the parts. A pad that reads high after being driven low is never reported safe. **Routing:** `avs-wire` ABI 1.1 adds `Route::Auto` (the headphones while the jack holds a plug, else the protected speakers), `Speaker::Protected`, `SpeakerHold::Released` (a speaker session is running) and `Output` in the status byte that was reserved; the device switches paths by taking the running one down first. A board whose record does not list the Pixelbook Go's two amplifiers (0x31, 0x32) has no speaker path (`SPEAKER-PROTECTION-UNAVAILABLE`). UNVERIFIED ON SILICON (named, not claimed): the order of `GLOBAL_EN` against the audio clock (the part has a clock monitor and auto-restart, so Linux's order works; ours was never run), the 50 ms/20 ms reset timing, the TDM slot masks beyond what the coreboot devicetree and Linux's machine driver say, the NHLT render format (2 ch, 16 of 16 bit, coreboot `nhlt/max98373.c:6-16`) matching the topology's blob format, the DSP volume stage honouring Q1.31 gains as Linux's `S32_MAX` unity (path.c:589) says, the level-to-loudness mapping, and the ceiling itself (a measurement of the cone replaces the arithmetic above).
