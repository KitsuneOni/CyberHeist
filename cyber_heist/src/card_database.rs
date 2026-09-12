use godot::classes::FileAccess;
use godot::prelude::*;
use std::collections::HashMap;
use crate::card_data::CardData;


pub fn parse_cards(text: &str) -> HashMap<String, CardData> {
    let cards: Vec<CardData> = ron::from_str(text)
        .expect("failed to parse cards.ron - check syntax");
    cards.into_iter().map(|c| (c.id.clone(), c)).collect()
}

pub fn load_card_database() -> HashMap<String, CardData> {
    let path = "res://data/cards.ron";

    let file = FileAccess::open(path, godot::classes::file_access::ModeFlags::READ)
        .expect("Failed to open cards.ron");

    let text = file.get_as_text().to_string();
    parse_cards(&text)  
}

#[derive(GodotClass)]
#[class(base=Node)]
pub struct CardDatabase {
    base: Base<Node>,
    cards: HashMap<String, CardData>,
}

#[godot_api]
impl INode for CardDatabase{
    fn init(base: Base<Node>) -> Self {
        Self{
            base,
            cards: HashMap::new(),
        }
    }

    fn ready(&mut self){
        self.cards = load_card_database();
        godot_print!("Loaded {} cards", self.cards.len());
    }
}

impl CardDatabase {
    pub fn get(&self, id: &str) -> Option<&CardData>{
        self.cards.get(id)
    }
    pub fn all(&self) -> impl Iterator<Item = &CardData>{
        self.cards.values()
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
    [
        (
            id: "vpn",
            name: "VPN",
            description: "End your turn - become intangible",
            cost: 2,
            noise_generated: -10,
            card_type: Skill,
            rarity: Common,
            keywords: [Intangible(1)],
        ),
    ]
    "#;

    #[test]
    fn parses_sample_cards() {
        let cards = parse_cards(SAMPLE);
        assert_eq!(cards.len(), 1);
        let vpn = cards.get("vpn").expect("strike should exist");
        assert_eq!(vpn.cost, 2);
        assert_eq!(vpn.keywords, vec![crate::card_data::Keyword::Intangible(1)]);
    }

    #[test]
    fn real_cards_file_parses() {
        let text = std::fs::read_to_string("../godot/data/cards.ron")
            .expect("could not find cards.ron — check the relative path");
        let cards = parse_cards(&text);
        assert!(!cards.is_empty());
    }
}