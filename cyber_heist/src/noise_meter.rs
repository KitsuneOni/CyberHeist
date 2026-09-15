use godot::prelude::*;

#[derive(GodotClass)]
#[class(base=Node)]
pub struct NoiseMeter {
    base: Base<Node>,

    #[export]
    pub noise: i32,

    #[export]
    pub max_noise: i32,
}

#[godot_api]
impl INode for NoiseMeter {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            noise: 0,
            max_noise: 100,
        }
    }
}

#[godot_api]
impl NoiseMeter {
    #[func]
    pub fn add_noise(&mut self, amount: i32) {
        self.noise = self.noise.saturating_add(amount).clamp(0, self.max_noise);
        godot_print!("Noise meter: {}/{}", self.noise, self.max_noise);
    }
}
