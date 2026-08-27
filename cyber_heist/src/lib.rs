//! CyberHeist GDExtension library for Godot 4.
//!
//! Game logic lives in this crate; the Godot project under `../godot` handles
//! scenes, UI and presentation. Add new gameplay modules here and register them
//! as Godot classes with `#[derive(GodotClass)]`.
//! 
//! Classes defined here:
//! - 'BridgeCheck' temporary demo proving the bridge between Godot and Rust works. Delete once real gameplay and classes are wired up
//! - 'PlayerState' - autoload singleton tracking player money and upgrades,
//! and applying the "caught" penalty (fine + lose this contract's upgrades).

use godot::prelude::*;
use godot::classes::Node;
use godot::builtin::{dict, VarDictionary};


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

/// Tracks player money and the upgrades earned so far during the current contract.
/// Exposed as an autoload singleton so GDScript can call it from anywhere
#[derive(GodotClass)]
#[class(base=Node)]
struct PlayerState {
    base: Base<Node>,
    money: i64,
    // Upgrades gained during the current contract only. Cleared on
    // apply_penalty' (caught) - permanent upgrades would need separate storage.
    upgrades: Vec<GString>,
}

#[godot_api]
impl INode for PlayerState {
    // called once when the autoload node enters the scene tree
    // starting values: no money, no upgrades yet.
    fn init(base: Base<Node>) -> Self {
        Self {base, money: 0, upgrades: Vec::new()}
    }
}

#[godot_api]
impl PlayerState {
    // Records an upgrade earned this contract e.g. called when the player picks a reward mid run
    #[func]
    fn add_upgrade(&mut self, name: GString) {
        self.upgrades.push(name);
    }

    // Called when the player is caught. Deducts 'fine' from money and wipes out any upgrades earned this contract
    // Returns what was lost so the CaugthtScreen can display it:
    // {"fine": int, "lost_upgrades": Array[String] }.
    #[func]
    fn apply_penalty(&mut self, fine: i64) -> VarDictionary {
        // take ownership of the current upgrades list and empty it out
        // drain(..) removes every element and hands them to lost
      let lost: Array<GString> = self.upgrades.drain(..).collect();
      self.money -= fine;

      // build the untyped dictionary GDScript expects back
      dict! {
          "fine" => fine,
          "lost_upgrades" => &lost,
      }
    }
}