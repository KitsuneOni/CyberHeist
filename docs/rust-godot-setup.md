# Rust + Godot Setup

How the project fits together and how to get it running.

For how we branch and merge, see workflow.md.

## How it works

The project is in two halves:

- `cyber_heist/` is the Rust crate. Game logic goes here - cards, decks, encounters, detection, saves.
- `godot/` is the Godot project. Scenes, UI, input, art and audio go here.

They are connected by gdext, which is the Rust binding for Godot 4. You might see it called godot-rust, it's the same thing. gdext is the Godot 4 version, gdnative was the old Godot 3 one which we don't use. The crate itself is just called `godot`.

It works like this:

1. `cargo build` compiles the Rust crate into a shared library (`libcyber_heist.so` on Linux, `cyber_heist.dll` on Windows, `.dylib` on Mac) in `cyber_heist/target/debug/`.
2. `godot/cyber_heist.gdextension` is a small text file that tells Godot where that library is on each platform.
3. When Godot starts it loads the library and registers every Rust type marked with `#[derive(GodotClass)]`. Those become real Godot node types, so they show up in the Add Node dialog and GDScript can call them like anything built in.

The main thing to remember is that Godot does not compile Rust. After you change a `.rs` file you have to run `cargo build` yourself and then reload the Godot project. If you skip that, your changes won't do anything.

## Installing Rust

Don't use the version from apt, it's too old. Our crate uses edition 2024 which needs Rust 1.85 or newer. Use rustup instead.

Linux and Mac:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Take the default options, then restart your terminal (or run `source ~/.cargo/env`) so `cargo` is on your PATH.

Windows: download rustup-init.exe from https://rustup.rs. When it asks, install the MSVC toolchain and the Visual Studio C++ Build Tools it offers. On Windows ARM64, also select the MSVC ARM64/ARM64EC build-tools component in the Visual Studio Installer. Without the build tools for your architecture, the extension won't link.

Check it worked:

```bash
cargo --version
```

You don't need to pick a Rust version yourself. `rust-toolchain.toml` in the repo root pins the exact one we all use, and rustup installs it automatically the first time you build. This is why rustup matters and an apt install doesn't work, apt ignores that file.

## Installing Godot

Get Godot 4.7.1 (the standard version, not .NET/Mono) from https://godotengine.org/download.

We should all be on the same version. The extension is built against the Godot 4.6 API and `cyber_heist.gdextension` sets `compatibility_minimum = 4.6`, so anything older than 4.6 won't load it.

## First build

Build the Rust side before you open Godot:

```bash
cd cyber_heist
cargo build
```

The first build downloads and compiles the Godot bindings so it takes a few minutes. After that it's seconds. It produces a library around 100MB, that's normal for a debug build and it's gitignored.

## Opening the project

In the Godot Project Manager click Import and pick `godot/project.godot`.

Open the `godot/` folder, not the root of the repo.

Press F5 to run. The permanent `main.tscn` bootstrap should open the contract
route map with Combat and Event choices. Select a route, complete the loaded
encounter, and confirm the map offers only encounters onward from that choice.
This path also proves the Rust extension and persistent run-state adapter loaded
successfully.

For the lifecycle and scene-integration API, see [game-flow.md](game-flow.md).

## Day to day

After editing any Rust file:

```bash
cd cyber_heist
cargo build
```

Then in Godot go to Project > Reload Current Project. The editor only picks up a rebuilt library when the project loads.

To run the tests (our Definition of Done needs these):

```bash
cd cyber_heist
cargo test
```

The tests are plain Rust and don't need Godot running, as long as the code being tested doesn't use Godot types. Keeping the rules logic in normal Rust structs, separate from the GodotClass wrappers, is what makes it testable.

## Adding a Rust class Godot can use

In `cyber_heist/src/lib.rs`, or a module you declare there:

```rust
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base=Node)]
struct DetectionMeter {
    #[export]              // shows up in the Inspector
    max_detection: i32,

    current: i32,
    base: Base<Node>,      // required, gives access to the node underneath
}

#[godot_api]
impl INode for DetectionMeter {
    fn init(base: Base<Node>) -> Self {
        Self { max_detection: 100, current: 0, base }
    }
}

#[godot_api]
impl DetectionMeter {
    #[func]                // makes it callable from GDScript
    fn raise(&mut self, amount: i32) {
        self.current = (self.current + amount).min(self.max_detection);
    }

    #[signal]              // a signal GDScript can connect to
    fn detection_maxed();
}
```

Run `cargo build`, reload the project, and `DetectionMeter` will be in the Add Node dialog. GDScript can then call `$DetectionMeter.raise(10)`.

The attributes you'll use most:

- `#[derive(GodotClass)]` registers the type with Godot
- `#[class(base=Node)]` sets what it extends (Node, Node2D, Control, Resource and so on)
- `#[func]` exposes a method to GDScript
- `#[export]` exposes a field in the Inspector
- `#[signal]` declares a signal
- `#[class(tool)]` also runs it in the editor, not just at runtime

As we add more, split things into modules like `src/card.rs` and `src/deck.rs` and declare them with `mod card;` in `lib.rs`, rather than letting `lib.rs` get huge.

## Troubleshooting

"Cannot get class 'X'", or a node turns into a placeholder - the library is out of date or was never built. Run `cargo build` and reload the project. This also happens if you add a new `#[derive(GodotClass)]` type and forget to rebuild.

The extension doesn't load at all - check that `cyber_heist/target/debug/libcyber_heist.so` (or `.dll`/`.dylib`) actually exists. The paths in `cyber_heist.gdextension` are relative to `godot/`, so `cyber_heist/` has to stay next to `godot/`.

A version mismatch error on load - your Godot is older than 4.6, update to 4.7.1.

Linker errors on Windows during `cargo build` - the MSVC C++ build tools are missing. Run rustup-init.exe again, or install "Desktop development with C++" from the Visual Studio Installer. On Windows ARM64, make sure the MSVC ARM64/ARM64EC build-tools component is selected too.

Godot crashes or misbehaves after you rebuild while it's open - close Godot before running `cargo build`. On Windows especially, the editor locks the DLL while it's open.

Your Rust changes don't seem to do anything - you didn't rebuild, or didn't reload the project. In that order.

## What's in the repo

```
CyberHeist/
├── cyber_heist/                    # Rust crate, game logic
│   ├── Cargo.toml                  # dependencies, godot = "0.5.5"
│   ├── Cargo.lock                  # committed so we all get the same versions
│   └── src/                        # cards, deck and contract run-state model
├── godot/                          # Godot project, open this folder
│   ├── project.godot
│   ├── cyber_heist.gdextension     # points Godot at the compiled library
│   ├── autoload/                   # persistent flow coordinator + run state
│   ├── scenes/
│   │   ├── main.tscn               # permanent bootstrap and ScreenHost
│   │   ├── contract_hub.tscn       # selectable contract route map
│   │   ├── combat.tscn             # draw-card combat entered from the map
│   │   └── placeholder_encounter.tscn # event/shop/elite placeholder
│   └── assets/                     # art and audio, empty for now
├── docs/
│   ├── game-flow.md                # lifecycle and scene integration
│   ├── rust-godot-setup.md         # this file
│   └── workflow.md                 # branching, reviews, merging, CI
├── .github/workflows/ci.yml        # builds and tests every push and PR
├── rust-toolchain.toml             # pins the Rust version
└── .gitignore
```

`main.tscn` is the stable bootstrap and must remain `run/main_scene`; feature
scenes are children of its `ScreenHost`. `BridgeCheck` remains available for
isolated diagnostics but is no longer part of the running scene tree.

`cyber_heist/target/` and `godot/.godot/` are gitignored. They're build output and Godot's local import cache, they regenerate on their own, don't commit them.
