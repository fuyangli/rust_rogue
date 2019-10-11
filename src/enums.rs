#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ItemType {
    Heal,
    Damage,
    FireBolt,
    Confuse,
    Scare,
    Merge
}

// pub enum Item {
//     HealPotion,
//     FireBall,
//     ConfusionSpell,
//     ScareSpell,
//     MergeSpell
// }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UseResult {
    UsedUp,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlayerAction {
    TookTurn,
    DidntTakeTurn,
    Exit,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DeathCallback {
    Player,
    Monster,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Ai {
    Basic,
    Confused {
        previous: Box<Ai>,
        turns: i32
    },
    Scared {
        previous: Box<Ai>,
        turns: i32
    }
}
