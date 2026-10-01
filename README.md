# Cookie Heap Lab

A native Rust cookie game with a safe, simulated 64-bit machine. Bake cookies, fulfil customer orders, race a baking rush, experiment with recipes, buy nine generator tiers, unlock upgrades and achievements, and prestige. Launch with `--debug` to inspect and modify the guest machine.

## Run

Install Rust 1.95 or newer, then:

```sh
cargo run -p cookie-heap-lab -- --help
cargo run -p cookie-heap-lab -- --seed 1234
cargo run -p cookie-heap-lab -- --debug --seed 1234
cargo run -p cookie-heap-lab -- --debug --load bakery.chls
```

Build the executable:

```sh
cargo build --release --locked
./target/release/cookie-heap-lab --debug
```

On Windows the executable is `target\release\cookie-heap-lab.exe`. Linux supports X11 and Wayland; rendering uses wgpu. Windows and macOS builds use the same codebase. The CI workflow builds and uploads a native executable on all three operating systems; those remote builds require running CI.

## Play and inspect

The main screen shows total cookies baked, cookies available to spend, click power, and passive production. Generator rows show production per unit and total production from your owned units. Upgrade purchases appear in the same right column as generator purchases.

- **Orders:** three customers reward baking, clicking, and generator ownership. All three progress together. Collecting a reward supplies a new order scaled to your bakery; rewards do not consume cookies.
- **Baking rush:** click 12–20 times within 20 simulated seconds. Collect a cookie bonus on success. Missing the deadline costs nothing, and you can retry. Successful rushes have a ten-second cooldown.
- **Recipes:** unlock Chocolate chip for 40 cookies (3× clicking) or Vanilla crunch for 80 (3× generator production). Unlocks are permanent within the saved game; switching is free, with one recipe active at a time. Click the recipe name below the production figures to open the recipe tab.
- **Golden batch:** choose a temporary 7× boost or an instant cookie shower. The shower reward scales with your production.

Goals, rewards, recipe multipliers, rush progress, and deadlines are ordinary guest bytes. The same scanner and hex editor can modify them. See the [activity layouts](docs/memory-layout.md#bakery-activities) for instructor reference.

Debug mode starts with the game visible. Open **Memory tools → Find a value…** for the scanner or **Inspect memory…** for a floating memory window. Tools can be moved, resized, grouped, and closed. No debugger panels are constructed during an ordinary launch. There is no GUI toggle to enable debugging.

The scanner accepts:

- Numbers: `127`, `0x7F`, or `0b1111111`, using 64 bits by default.
- Text: exact UTF-8 strings such as `Crumb & Co.`.
- Hex bytes: patterns such as `43 72 75 6D 62`, matched in the given byte order.

Run a first scan, change the value in the game, then narrow results. Click **Inspect** beside a result to open its memory address. Scan options contain integer widths, alignment, unknown initial values, and comparison filters. String and byte filters retain the original pattern length.

In Memory, **double-click a hex cell, type hex byte pairs, and press Enter**. A sequence such as `E8 03 00 00 00 00 00 00` writes the little-endian integer 1000 starting at that cell. Escape cancels. All edits pass through the VM's checked memory API. Additional tools provide typed inspection, pointer following, watches, write provenance, and snapshots/diffs. Allocation information is available in Inspector; there is no separate heap panel.

## Save and load

Use **File → Save Game As…** (`Ctrl/Cmd+Shift+S`) to select an explicit destination. **File → Load Game…** (`Ctrl/Cmd+O`) validates the file before asking to replace the session. `--load FILE` loads at startup; it conflicts with `--seed`.

Every launch without `--load` starts fresh, even if save files exist. There is no autosave, save on exit, implicit restoration, or offline production. Saves include mapped memory, allocator metadata and free list, seed/RNG state, guest tick, pointers, fractional production, achievements, and effect deadlines. Debugger windows, watches, scans, and diffs are excluded. Corrupted guest state can be saved and loaded exactly, including pointers that subsequently fault.

Order progress, recipe unlocks, and active rushes resume exactly. Older saves are supported: loading adds the new activity allocations while preserving existing game bytes, heap addresses, RNG, and simulated time.

## Architecture and checks

See [architecture](docs/architecture.md) and [guest memory layout](docs/memory-layout.md).

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Tests cover address/permission faults, atomic memory transfers, little-endian pointers, allocator reuse/coalescing/invariants, deliberate corruption, deterministic economy, CLI gating, exact save continuation and malformed saves, scanner formats, pointer input, inline hex editing, and contrast in both themes. UI tests run headlessly; they do not control the desktop.
