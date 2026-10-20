# CyberHeist

A roguelike deckbuilder where you play a freelance hacker taking corporate contracts. Each run is a network intrusion: pick a contract, work your way across a branching map of encounters, play cards to get past the security systems, and keep your noise low enough that nobody notices you were there.

Made for COMP602 Software Development Practice at AUT, Semester 2 2026.

Game logic is written in Rust and runs inside the Godot 4 engine through [gdext](https://github.com/godot-rust/gdext) (godot-rust).

## Requirements

| Tool | Version | Notes |
|---|---|---|
| [Godot](https://godotengine.org/download) | 4.7.1 (standard, not .NET) | 4.6 is the oldest version that will load the extension |
| [Rust](https://rustup.rs) via rustup | 1.97.1 | Pinned in `rust-toolchain.toml`, rustup installs it for you |
| Git | any recent | |
| MSVC C++ Build Tools | | Windows only, needed to link the extension |

Install Rust with rustup, not apt. The apt version is too old for edition 2024 and ignores the pinned toolchain.

## Dependencies

Rust crates (from `cyber_heist/Cargo.toml`, versions locked in `Cargo.lock`):

| Crate | Locked version | Used for |
|---|---|---|
| `godot` | 0.5.5 | Rust bindings for Godot 4 (gdext) |
| `rand` | 0.9.5 | Shuffling, map generation, shop stock |
| `serde` | 1.0.229 | Serialising game data |
| `ron` | 0.8.1 | Reading card and game data files |
| `serde_json` | 1.0.154 | Save files |

Dev tooling that comes with the pinned toolchain: `rustfmt` and `clippy`.

Optional: Python 3 with `numpy`, only if you want to regenerate the sound effects and music (see [Example usage](#example-usage)).

Third-party fonts and icons are listed with their licences in [`godot/assets/CREDITS.md`](godot/assets/CREDITS.md).

## Getting started

```bash
git clone https://github.com/KitsuneOni/CyberHeist.git
cd CyberHeist/cyber_heist
cargo build
```

The first build takes a few minutes because it compiles the Godot bindings. After that it only takes seconds.

## Running the game

1. Build the Rust library first (`cargo build` in `cyber_heist/`).
2. Open the Godot Project Manager, click **Import** and select `godot/project.godot`. Open the `godot/` folder, not the repo root.
3. Press **F5** to run.

From a terminal, if Godot is on your PATH:

```bash
godot --path godot
```

Godot does not compile Rust. After changing any `.rs` file, run `cargo build` again and then use **Project > Reload Current Project**. Close Godot before rebuilding on Windows, because the editor locks the DLL.

## Building

Debug build, which is what the editor uses:

```bash
cd cyber_heist
cargo build
```

Release build:

```bash
cd cyber_heist
cargo build --release
```

`godot/cyber_heist.gdextension` points Godot at `target/debug/` for the editor and `target/release/` for exported builds. To make a standalone game, run the release build, then in Godot go to **Project > Export**, add a preset for your platform and export. Export presets are gitignored because they can contain local paths.

## Testing

Rust unit tests, which are needed for our Definition of Done:

```bash
cd cyber_heist
cargo test
```

The same checks CI runs on every push and pull request:

```bash
cd cyber_heist
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build
cargo test
```

## Example usage

### Playing a run

1. Launch the game and choose **New contract** from the main menu.
2. Flick through the three contract offers with **Previous offer** and **Next offer**. Each offer shows its target, credit reward and number of zones. Pick one with **Accept contract**.
3. On the contract map, choose your next node. Nodes can be combat, elite, event or shop.
4. In combat, select a card to see its details, then **Play Card**. Each card costs energy and makes noise. Bring the security system's integrity to 0 to win. Press **End Turn** when you're done, and the sentry acts. The intent panel shows what it will do next.
5. If your noise fills the meter, you're caught and the run ends.
6. Spend credits at **DARK WEB // SHOP** nodes on new cards, **SCRUB LOGS** to lower noise, or **REROLL STOCK**.
7. Beat the boss of the final zone to win the contract and see your run summary.

The game saves itself at safe points, so you can quit and pick up again with **Continue run**. Volume settings are under **Settings** on the main menu. The game uses the mouse throughout.

Save data is kept in Godot's user folder:

| File | Contents |
|---|---|
| `user://cyberheist_save.json` | Current run and credits |
| `user://settings.cfg` | Audio settings |

On Linux this is `~/.local/share/godot/app_userdata/CyberHeist/`, and on Windows it's `%APPDATA%\Godot\app_userdata\CyberHeist\`. Delete `cyberheist_save.json` to start fresh.

### Running the stress tests

These are the `#[ignore]` tests that hit the noise meter with extreme and random values:

```bash
cd cyber_heist
cargo test -- --ignored
```

### Running a Godot scene test headless

The tests in `godot/tests/` load the real scenes and the Rust extension, so build the library first:

```bash
cd cyber_heist && cargo build && cd ..
godot --headless --path godot --import
godot --headless --path godot -s res://tests/combat_smoke_test.gd
```

Swap in any other test file, for example `res://tests/save_continue_test.gd` or `res://tests/shop_test.gd`.

### Regenerating the audio

All sound effects and music are synthesised by a script, so there is nothing to license:

```bash
pip install numpy
python3 tools/generate_audio.py
```

## Project structure

```
CyberHeist/
├── cyber_heist/          # Rust crate: cards, deck, combat, noise, shop, saves
│   └── src/
├── godot/                # Godot project: scenes, UI, art, audio
│   ├── autoload/         # FlowCoordinator, run state, audio
│   ├── scenes/           # main menu, map, combat, shop, events, end screens
│   ├── tests/            # headless scene tests
│   └── assets/           # fonts, icons, audio
├── docs/                 # setup guide, game flow, team workflow
├── tools/                # audio generator
└── .github/workflows/    # CI and stress tests
```

## Troubleshooting

- **"Cannot get class" or nodes show as placeholders.** The library is out of date or hasn't been built. Run `cargo build` and reload the project.
- **The extension doesn't load at all.** Check that `cyber_heist/target/debug/` contains `libcyber_heist.so`, `cyber_heist.dll` or `libcyber_heist.dylib`. `cyber_heist/` has to stay next to `godot/`.
- **A version error on load.** Your Godot is older than 4.6. Update to 4.7.1.
- **Linker errors on Windows.** Install "Desktop development with C++" from the Visual Studio Installer.

For more detail, see [docs/rust-godot-setup.md](docs/rust-godot-setup.md). Branching and PR rules are in [docs/workflow.md](docs/workflow.md).

## Authors

Team W204, COMP602 Semester 2 2026:

| Name | Role |
|---|---|
| Adam Boyce | Product Owner, Developer |
| Neel Kolhe | Scrum Master, Developer |
| Hunter Yates | Developer |
| Gerard Tibbles | Developer |
