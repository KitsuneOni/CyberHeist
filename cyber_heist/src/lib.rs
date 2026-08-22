//! CyberHeist GDExtension library for Godot 4.
//!
//! Game logic lives in this crate; the Godot project under `../godot` handles
//! scenes, UI and presentation. Add new gameplay modules here and register them
//! as Godot classes with `#[derive(GodotClass)]`.

use godot::prelude::*;

struct CyberHeistExtension;

#[gdextension]
unsafe impl ExtensionLibrary for CyberHeistExtension {}

/// Temporary node that proves the Rust <-> Godot bridge is wired up correctly.
///
/// Attach it to a scene (or use `godot/scenes/main.tscn`) and run the project:
/// the `_ready` message should appear in Godot's Output panel. Once real
/// gameplay classes exist this can be deleted along with `main.tscn`.
#[derive(GodotClass)]
#[class(base=Node)]
struct BridgeCheck {
    base: Base<Node>,
}

#[godot_api]
impl INode for BridgeCheck {
    fn init(base: Base<Node>) -> Self {
        Self { base }
    }

    fn ready(&mut self) {
        godot_print!("CyberHeist: Rust bridge is live (BridgeCheck::ready).");
    }
}

#[godot_api]
impl BridgeCheck {
    /// Callable from GDScript to confirm calls cross the bridge, e.g.
    /// `print($BridgeCheck.ping())`.
    #[func]
    fn ping(&self) -> GString {
        "pong from Rust".into()
    }
}
