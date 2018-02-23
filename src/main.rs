extern crate tcod;
extern crate rand;
extern crate bresenham;

use std::cmp;

use tcod::console::*;
use tcod::colors::{self, Color};
use tcod::map::{Map as FovMap, FovAlgorithm};
use rand::Rng;
use std::ascii::AsciiExt;
use tcod::input::{self, Event, Mouse};
use bresenham::Bresenham;

const PLAYER: usize = 0;

// actual size of the window
const SCREEN_WIDTH: i32 = 80;
const SCREEN_HEIGHT: i32 = 50;

// size of the map
const MAP_WIDTH: i32 = 800 + SCREEN_WIDTH + 1;
const MAP_HEIGHT: i32 = 450 + SCREEN_HEIGHT + 1; 

// Panel
const BAR_WIDTH: i32 = 20;
const PANEL_HEIGHT: i32 = 7;
const PANEL_Y: i32 = SCREEN_HEIGHT - PANEL_HEIGHT;

// Mensagens
const MSG_X: i32 = BAR_WIDTH + 2;
const MSG_WIDTH: i32 = SCREEN_WIDTH - BAR_WIDTH - 2;
const MSG_HEIGHT: usize = PANEL_HEIGHT as usize - 1;

//parameters for dungeon generator
const ROOM_MAX_SIZE: i32 = 35;
const ROOM_MIN_SIZE: i32 = 5;
const MAX_ROOMS: i32 = (MAP_HEIGHT + MAP_WIDTH) / 10;
const MAX_ROOMS_MONSTERS: i32 = 3;

const MAX_ROOMS_ITEMS: i32 = 3;

const INVENTORY_WIDTH: i32 = 50;

const FOV_ALGO: FovAlgorithm = FovAlgorithm::Shadow;  // default FOV algorithm
const FOV_LIGHT_WALLS: bool = true;  // light walls or not
const TORCH_RADIUS: i32 = 10;

const LIMIT_FPS: i32 = 20;  // 20 frames-per-second maximum

const COLOR_DARK_WALL: Color = Color { r: 44, g: 62, b: 80 };
const COLOR_LIGHT_WALL: Color = Color { r: 52, g: 73, b: 94 };
const COLOR_DARK_GROUND: Color = Color { r: 153, g:45, b: 34 };
const COLOR_LIGHT_GROUND: Color = Color { r: 231, g: 76, b: 60 };

type Map = Vec<Vec<Tile>>;
type Messages = Vec<(String, Color)>;

// ENUMS
#[derive(Clone, Copy, Debug, PartialEq)]
enum ItemType {
    Heal,
    Damage,
    FireBolt,
    Confuse,
    Scare,
    Merge
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum UseResult {
    UsedUp,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PlayerAction {
    TookTurn,
    DidntTakeTurn,
    Exit,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DeathCallback {
    Player,
    Monster,
}

// STRUCTS

struct Tcod {
    root: Root,
    con: Offscreen,
    panel: Offscreen,
    fov: FovMap,
    mouse: Mouse,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Fighter {
    max_hp: i32,
    hp: i32,
    defense: i32,
    power: i32,
    on_death: DeathCallback,
}

#[derive(Clone, Debug, PartialEq)]
enum Ai {
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

#[derive(Clone, Copy, Debug)]
struct Tile {
    blocked: bool,
    explored: bool,
    block_sight: bool,
    char: char,
    light_color: Color,
    dark_color: Color
}

#[derive(Clone, Copy, Debug)]
struct Rect {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    connections: i32,
}

struct Circle {
    radius: i32,
    x: i32,
    y: i32
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Item {
    item_type: ItemType,
    amount: u32,
    range: u32
}

#[derive(Clone, Debug, PartialEq)]
struct Object {
    x: i32,
    y: i32,
    char: char,
    color: Color,
    name: String,
    blocks: bool,
    alive: bool,
    fighter: Option<Fighter>,
    ai: Option<Ai>,
    item: Option<Item>,
}

// IMPLEMENTATIONS

impl DeathCallback {
    fn callback(self, object: &mut Object, messages: &mut Messages) {
        use DeathCallback::*;
        let callback: fn(&mut Object, &mut Messages) = match self {
            Player => player_death,
            Monster => monster_death,
        };
        callback(object, messages);
    }
}

impl Tile {
    pub fn new(blocked: bool, explored: bool, block_sight: bool, char: char, light_color: Color, dark_color: Color) -> Self {
        Tile {blocked: blocked, explored: explored, block_sight: block_sight, char: char, light_color: light_color, dark_color: dark_color}
    }
    pub fn empty() -> Self {
        Tile{blocked: false, explored: false, block_sight: false, char: ' ', light_color: COLOR_LIGHT_GROUND, dark_color: COLOR_DARK_GROUND }
    }

    pub fn wall() -> Self {
        Tile{blocked: true, explored: false, block_sight: true, char: '#', light_color: COLOR_LIGHT_WALL, dark_color: COLOR_DARK_WALL }
    }

    pub fn water() -> Self {
        Tile::new(false, false, false, '~', colors::BLUE, colors::DARK_BLUE)
    }
}


impl Circle {
    pub fn new(x: i32, y: i32, radius: i32) -> Self {
        Circle { x: x, y: y, radius: radius }
    }

    pub fn center(&self) -> (i32, i32) {
        let center_x = (self.x + self.radius) / 2;
        let center_y = (self.y + self.radius) / 2;
        (center_x, center_y)
    }

}


impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Rect { x1: x, y1: y, x2: x + w, y2: y + h, connections: 0 }
    }

    pub fn center(&self) -> (i32, i32) {
        let center_x = (self.x1 + self.x2) / 2;
        let center_y = (self.y1 + self.y2) / 2;
        (center_x, center_y)
    }

    pub fn intersects_with(&self, other: &Rect) -> bool {
        // returns true if this rectangle intersects with another one
        (self.x1 <= other.x2) && (self.x2 >= other.x1) &&
            (self.y1 <= other.y2) && (self.y2 >= other.y1)
    }
}

impl Item {
    pub fn new(item_type: ItemType, amount: u32, range: u32) -> Self {
        Item {
            item_type: item_type,
            amount: amount,
            range: range
        }
    }
    
}

impl Object {
    pub fn new(x: i32, y: i32, char: char, name: String, color: Color, blocks: bool) -> Self {
        Object {
            x: x,
            y: y,
            char: char,
            color: color,
            name: name,
            blocks: blocks,
            alive: false,
            fighter: None,
            ai: None,
            item: None
        }
    }

    /// set the color and then draw the character that represents this object at its position
    pub fn draw(&self, con: &mut Console) {
        con.set_default_foreground(self.color);
        con.put_char(self.x, self.y, self.char, BackgroundFlag::None);
    }

    /// Erase the character that represents this object
    pub fn clear(&self, con: &mut Console) {
        con.put_char(self.x, self.y, ' ', BackgroundFlag::None);
    }

    pub fn pos(&self) -> (i32, i32) {
        (self.x, self.y)
    }
    pub fn set_pos(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
    }

    pub fn distance_to(&self, other: &Object) -> f32 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        ((dx.pow(2) + dy.pow(2)) as f32).sqrt()
    }

    pub fn heal(&mut self, amount: i32) {
        if let Some(fighter) = self.fighter.as_mut() {
            fighter.hp += amount;
            if fighter.hp > fighter.max_hp {
                fighter.hp = fighter.max_hp;
            }
        }
    }

    pub fn take_damage(&mut self, damage: i32, messages: &mut Messages) {
        // apply damage if possible
        if let Some(fighter) = self.fighter.as_mut() {
            if damage > 0 {
                fighter.hp -= damage;
            }
            else {
                fighter.hp = 0;
            }
        }

        if let Some(fighter) = self.fighter {
            if fighter.hp <= 0 {
                self.alive = false;
                fighter.on_death.callback(self, messages);
            }
        }
    }

    pub fn attack(&mut self, target: &mut Object, messages: &mut Messages) {
    // a simple formula for attack damage
        let damage = self.fighter.map_or(0, |f| f.power) - target.fighter.map_or(0, |f| f.defense);
        if damage > 0 {
            // make the target take some damage
            message(messages, format!("{} attacks {} for {} hit points.", self.name, target.name, damage), colors::RED);
            target.take_damage(damage, messages);
        } else {
            message(messages, format!("{} attacks {} but it has no effect!", self.name, target.name), colors::RED);
        }
    }

    pub fn merge(&mut self, target: &mut Object) -> Object {
        let defense = ( ((self.fighter.map_or(0, |f| f.defense) + target.fighter.map_or(0, |f| f.defense)) as f32) * 0.75) as i32;
        let power = (((self.fighter.map_or(0, |f| f.power) + target.fighter.map_or(0, |f| f.power)) as f32) * 0.75)  as i32;
        let max_hp = (((self.fighter.map_or(0, |f| f.max_hp) + target.fighter.map_or(0, |f| f.max_hp)) as f32) * 0.75)  as i32; 
        let hp = (((self.fighter.map_or(0, |f| f.hp) + target.fighter.map_or(0, |f| f.hp)) as f32) * 0.75) as i32;

        let c = self.char;
        let color = target.color;
        let name = self.name.clone() + " "  + &target.name;

        let mut object = Object::new(self.x, self.y, c, name, color, self.blocks);
        object.fighter = Some(Fighter {
            defense: defense,
            hp: hp,
            max_hp: max_hp,
            power: power,
            on_death: DeathCallback::Player
        });

        object.ai = self.ai.clone();
        object.alive = true;
        
        object
    }

}

// FUNTIONS

fn pick_item_up(object_id: usize, objects: &mut Vec<Object>, inventory: &mut Vec<Object>,
                messages: &mut Messages) {
    if inventory.len() >= 26 {
        message(messages,
                format!("Your inventory is full, cannot pick up {}.", objects[object_id].name),
                colors::COPPER);
    } else {
        let item = objects.swap_remove(object_id);
        message(messages, format!("You picked up a {}!", item.name), colors::DARK_GREEN);
        inventory.push(item);
    }
}

fn player_death(player: &mut Object, messages: &mut Messages) {
    // the game ended!
    message(messages, "You died!", colors::RED);

    // for added effect, transform the player into a corpse!
    player.char = '%';
    player.color = colors::DARK_RED;
}

fn monster_death(monster: &mut Object, messages: &mut Messages) {
    // transform it into a nasty corpse! it doesn't block, can't be
    // attacked and doesn't move
    message(messages, format!("{} is dead!", monster.name), colors::GREEN);
    monster.char = 'x';
    monster.color = colors::DARK_RED;
    monster.blocks = false;
    monster.fighter = None;
    monster.ai = None;
    monster.name = format!("remains of {}", monster.name);
}

fn mut_two<T>(first_index: usize, second_index: usize, items: &mut [T]) -> (&mut T, &mut T) {
    assert!(first_index != second_index);
    let split_at_index = cmp::max(first_index, second_index);
    let (first_slice, second_slice) = items.split_at_mut(split_at_index);
    if first_index < second_index {
        (&mut first_slice[first_index], &mut second_slice[0])
    } else {
        (&mut second_slice[0], &mut first_slice[second_index])
    }
}

fn ai_take_turn(monster_id: usize, map: &Map, objects: &mut [Object], fov_map: &FovMap, messages: &mut Messages) {
    use Ai::*;
    if let Some(ai) = objects[monster_id].ai.take() {
        let new_ai = match ai {
            Basic => ai_basic(monster_id, map, objects, fov_map, messages),
            Confused{previous, turns} =>    ai_confused(monster_id, map, objects, messages, previous, turns),
            Scared{previous, turns} =>      ai_scared(monster_id, map, objects, fov_map, messages, previous, turns)
        };
        objects[monster_id].ai = Some(new_ai);
    }
}

fn ai_basic(monster_id: usize, map: &Map, objects: &mut [Object], fov_map: &FovMap, messages: &mut Messages) -> Ai {
    
    let (monster_x, monster_y) = objects[monster_id].pos();


    if fov_map.is_in_fov(monster_x, monster_y) {
        if objects[monster_id].distance_to(&objects[PLAYER]) >= 2.0 {
            // move towards player if far away
            let (player_x, player_y) = objects[PLAYER].pos();
            move_towards(monster_id, player_x, player_y, map, objects);
            //move_away(monster_id, player_x, player_y, map, objects);
        } else if objects[PLAYER].fighter.map_or(false, |f| f.hp > 0) {
            // close enough, attack! (if the player is still alive.)
            let (monster, player) = mut_two(monster_id, PLAYER, objects);
            monster.attack(player, messages);
        }
    }
    Ai::Basic
}

fn ai_scared(monster_id: usize, map: &Map, objects: &mut [Object], fov_map: &FovMap, messages: &mut Messages, previous: Box<Ai>, turns: i32) -> Ai {
    
    if turns > 0 {
        let (monster_x, monster_y) = objects[monster_id].pos();
        if fov_map.is_in_fov(monster_x, monster_y) {
            
            let (player_x, player_y) = objects[PLAYER].pos();
            move_away(monster_id, player_x, player_y, map, objects);
            return Ai::Scared{previous: previous, turns: turns - 1};
        }
    }
    *previous
}


fn ai_confused(monster_id: usize, map: &Map, objects: &mut [Object], messages: &mut Messages,
               previous: Box<Ai>, turns: i32) -> Ai {
    if turns >= 0 { 
        move_by(monster_id,
                rand::thread_rng().gen_range(-1, 2),
                rand::thread_rng().gen_range(-1, 2),
                map,
                objects);
        Ai::Confused{previous: previous, turns: turns - 1}
    } else {  // restore the previous AI (this one will be deleted)
        message(messages, format!("The {} is no longer confused!",
                                  objects[monster_id].name),
                colors::RED);
        *previous
    }
}


fn move_by(id: usize, dx: i32, dy: i32, map: &Map, objects: &mut [Object]) {
    let (x, y) = objects[id].pos();
    if !is_blocked(x + dx, y + dy, map, objects) {
        objects[id].set_pos(x + dx, y + dy);
    }
    else {
        
    }
}

fn create_water(room: Rect, map: &mut Map) {

    let w = rand::thread_rng().gen_range(0, (room.x2 - room.x1).abs());
    let h = rand::thread_rng().gen_range(0, (room.y2 - room.y1).abs());
    // random position without going out of the boundaries of the map
    let pos_x = rand::thread_rng().gen_range(room.x1, room.x2 + 1);
    let pos_z = rand::thread_rng().gen_range(room.y1, room.y2 + 1);

    let water = Rect::new(pos_x, pos_z, w, h);
    for x in (water.x1 + 1)..water.x2 {
        for y in (water.y1 + 1)..water.y2 {
            map[x as usize][y as usize] = Tile::new(false, false, false, '~', colors::BLUE, colors::DARK_BLUE);
        }
    }
}

fn create_circle(circle: Circle, map: &mut Map){
    for x in (circle.x + 1)..(circle.radius + circle.x + 1) {
        for y in (circle.y + 1)..(circle.radius + circle.y + 1) {
            if (x * x) + (y*y) < circle.radius.pow(2) / 2 {
                println!("Deu");
                map[x as usize][y as usize] = Tile::water();
            }
        }
    }
}

fn create_room(room: Rect, map: &mut Map) {
    // go through the tiles in the rectangle and make them passable
    for x in (room.x1 + 1)..room.x2 {
        for y in (room.y1 + 1)..room.y2 {
            map[x as usize][y as usize] = Tile::empty();
        }
    }
}

fn create_h_tunnel(x1: i32, x2: i32, y: i32, map: &mut Map) {
    for x in cmp::min(x1, x2)..(cmp::max(x1, x2) + 1) {
        map[x as usize][y as usize] = Tile::empty();
        map[(x + 1) as usize][(y + 1) as usize] = Tile::empty();
        map[(x + 1) as usize][y as usize] = Tile::empty();
        map[x as usize][(y + 1) as usize] = Tile::empty();
    }
}

fn create_v_tunnel(y1: i32, y2: i32, x: i32, map: &mut Map) {
    for y in cmp::min(y1, y2)..(cmp::max(y1, y2) + 1) {
        map[x as usize][y as usize] = Tile::empty();
        map[(x + 1) as usize][(y + 1) as usize] = Tile::empty();
        map[(x + 1) as usize][y as usize] = Tile::empty();
        map[x as usize][(y + 1) as usize] = Tile::empty();
    }
}

fn create_d_tunnel(y1: i32, y2: i32, x1: i32, x2: i32, map: &mut Map) {
    for (x,y) in Bresenham::new((x1 as isize, y1 as isize), (x2 as isize, y2 as isize)) {
        map[x as usize][y as usize] = Tile::empty();
        map[(x + 1) as usize][(y + 1) as usize] = Tile::empty();
        map[(x + 1) as usize][y as usize] = Tile::empty();
        map[x as usize][(y + 1) as usize] = Tile::empty();
    }
}

fn make_map(objects: &mut Vec<Object>) -> (Map) {

    let mut map = vec![vec![Tile::wall(); MAP_HEIGHT as usize]; MAP_WIDTH as usize];

    let mut rooms = vec![];

    for _ in 0..MAX_ROOMS {
        

        // random width and height
        let w = rand::thread_rng().gen_range(ROOM_MIN_SIZE, ROOM_MAX_SIZE + 1);
        let h = rand::thread_rng().gen_range(ROOM_MIN_SIZE, ROOM_MAX_SIZE + 1);

        // random position without going out of the boundaries of the map
        let x = rand::thread_rng().gen_range(0, MAP_WIDTH - w);
        let y = rand::thread_rng().gen_range(0, MAP_HEIGHT - h);

        let mut new_room = Rect::new(x, y, w, h);
        
        // run through the other rooms and see if they intersect with this one
        let failed = rooms.iter().any(|other_room| new_room.intersects_with(other_room));

        if !failed {
            // this means there are no intersections, so this room is valid

            // "paint" it to the map's tiles
            create_room(new_room, &mut map);

            
            

                create_circle( Circle{
                    x: x,
                    y: y,
                    radius: (w + h) /2
                }, &mut map);

            

            place_objects(new_room, objects, &mut map);

            // center coordinates of the new room, will be useful later
            let (new_x, new_y) = new_room.center();

            if rooms.is_empty() {
                // this is the first room, where the player starts at
                objects[PLAYER].set_pos(new_x, new_y);
            } 

            // finally, append the new room to the list
            rooms.push(new_room);
        }
    }

    for i in 0..rooms.len() {

        if i > (rooms.len() - 3) {
            continue;
        }
        let mut new_room = rooms[i as usize + 1];
        let (new_x, new_y) = new_room.center();
        let mut previous_room = rooms[i as usize];
        let (prev_x, prev_y) = previous_room.center();

        if previous_room.connections < 2 {
            // toss a coin (random bool value -- either true or false)
            let chance = rand::random::<f32>();
            if chance <= 0.33 {
                create_d_tunnel(prev_y, new_y, prev_x, new_x, &mut map);
            } else if chance <= 0.66 {
                create_h_tunnel(prev_x, new_x, prev_y, &mut map);
                create_v_tunnel(prev_y, new_y, new_x, &mut map);
            }
            else {
                create_v_tunnel(prev_y, new_y, prev_x, &mut map);
                create_h_tunnel(prev_x, new_x, new_y, &mut map);
            }

            if rand::random::<bool>() {
                create_water(new_room, &mut map);
            }

            previous_room.connections += 1;
            new_room.connections += 1;
        }
        // println!("Previous: {:?}", previous_room);
        // println!("New: {:?}", new_room);
    }

    (map)
}

fn render_all(tcod: &mut Tcod, objects: &[Object], map: &mut Map,
              fov_recompute: bool,messages: &Messages, camera: &mut (i32, i32)) {


    let vec = {
        let mut x = camera.0 - (SCREEN_WIDTH / 2);
        if x < 1 {
            x = 0;
        }
        let mut y = camera.1 - (SCREEN_HEIGHT / 2);
        if y < 1 {
            y = 0;
        }
        (x, y)
    };
    if fov_recompute {
        // recompute FOV if needed (the player moved or something)
        let player = &objects[PLAYER];
        tcod.fov.compute_fov(player.x, player.y, TORCH_RADIUS, FOV_LIGHT_WALLS, FOV_ALGO);
        for y in 0..MAP_HEIGHT {
            for x in 0..MAP_WIDTH {
                let visible = tcod.fov.is_in_fov(x, y);
                let mut tile =  map[x as usize][y as usize];
                let wall = tile.block_sight;
                let mut color = match (visible, wall) {
                     // outside of field of view:
                     (false, true) => tile.light_color,
                     (false, false) => tile.dark_color,
                     // inside fov:
                     (true, true) => colors::lerp(tile.light_color, tile.dark_color, ((((x - player.x).pow(2) + (y - player.y).pow(2)) as f32).sqrt() / TORCH_RADIUS as f32).powi(2)),
                     (true, false) => colors::lerp(tile.light_color, tile.dark_color, ((((x - player.x).pow(2) + (y - player.y).pow(2)) as f32).sqrt() / TORCH_RADIUS as f32).powi(2)),
                };

                let explored = &mut map[x as usize][y as usize].explored;
                if visible {
                    // since it's visible, explore it
                    *explored = true;
                }
                if *explored {
                    // show explored tiles only (any visible tile is explored already)
                    tcod.con.put_char_ex(x, y, tile.char,
                        Color {
                            r: std::cmp::max(color.r as i16 / 2, 0) as u8,
                            g: std::cmp::max(color.g as i16 / 2, 0) as u8,
                            b: std::cmp::max(color.b as i16 / 2, 0) as u8,
                        }, 
                        color);
                    tcod.con.set_char_background(x, y, color, BackgroundFlag::Set);
                }
            }
        }

    

        let mut to_draw : Vec<_> = objects.iter().filter(|o| tcod.fov.is_in_fov(o.x, o.y)).collect();
        to_draw.sort_by(|o1, o2| {
            o1.blocks.cmp(&o2.blocks)
        });
        for object in &to_draw {
            if tcod.fov.is_in_fov(object.x, object.y) {
                object.draw(&mut tcod.con);
            }
        }

        

        blit(&mut tcod.con, vec, (MAP_WIDTH, MAP_HEIGHT), &mut tcod.root, (0, 0), 1.0, 1.0);

        tcod.panel.set_default_background(colors::BLACK);
        tcod.panel.clear();

        let mut y = MSG_HEIGHT as i32;
        for &(ref msg, color) in messages.iter().rev() {
            let msg_height = tcod.panel.get_height_rect(MSG_X, y, MSG_WIDTH, 0, msg);
            y -= msg_height;
            if y < 0 {
                break;
            }
            tcod.panel.set_default_foreground(color);
            tcod.panel.print_rect(MSG_X, y, MSG_WIDTH, 0, msg);
        }

        // show the player's stats
        let hp = objects[PLAYER].fighter.map_or(0, |f| f.hp);
        let max_hp = objects[PLAYER].fighter.map_or(0, |f| f.max_hp);
        render_bar(&mut tcod.panel, 1, 1, BAR_WIDTH, "HP", hp, max_hp, colors::LIGHT_RED, colors::DARKER_RED);

        tcod.panel.set_default_foreground(colors::LIGHT_GREY);
        tcod.panel.print_ex(1, 0, BackgroundFlag::None, TextAlignment::Left, get_names_under_mouse(tcod.mouse, objects, &mut tcod.fov));

        blit(&mut tcod.panel, (0, 0), (MAP_WIDTH, PANEL_HEIGHT), &mut  tcod.root, (0, PANEL_Y), 1.0, 1.0);

    }

    
    
}

fn move_away(id: usize, target_x: i32, target_y: i32, map: &Map, objects: &mut [Object]) {
    // vector from this object to the target, and distance
    let dx = target_x - objects[id].x;
    let dy = target_y - objects[id].y;
    let distance = -((dx.pow(2) + dy.pow(2)) as f32).sqrt();

    // normalize it to length 1 (preserving direction), then round it and
    // convert to integer so the movement is restricted to the map grid
    let dx = (dx as f32 / distance).round() as i32;
    let dy = (dy as f32 / distance).round() as i32;
    move_by(id, dx, dy, map, objects);
}

fn move_towards(id: usize, target_x: i32, target_y: i32, map: &Map, objects: &mut [Object]) {
    // vector from this object to the target, and distance
    let dx = target_x - objects[id].x;
    let dy = target_y - objects[id].y;
    let distance = ((dx.pow(2) + dy.pow(2)) as f32).sqrt();

    // normalize it to length 1 (preserving direction), then round it and
    // convert to integer so the movement is restricted to the map grid
    let dx = (dx as f32 / distance).round() as i32;
    let dy = (dy as f32 / distance).round() as i32;

    move_by(id, dx, dy, map, objects);
}

fn player_move_or_attack(dx: i32, dy: i32, map: &Map, objects: &mut [Object], messages: &mut Messages) -> Option<Object> {
    // the coordinates the player is moving to/attacking
    let x = objects[PLAYER].x + dx;
    let y = objects[PLAYER].y + dy;

    // try to find an attackable object there
    let target_id = objects.iter().position(|object| {
        object.fighter.is_some() && object.pos() == (x, y)
    });
    // attack if target found, move otherwise
    match target_id {
        Some(target_id) => {
            let (player, target) = mut_two(PLAYER, target_id, objects);
            player.attack(target, messages);
            return None;

            // let new_player = player.merge(target);
            // return Some(new_player);
        }
        None => {
            move_by(PLAYER, dx, dy, map, objects);
            return None;
        }
    }
    
}

fn handle_keys(key: tcod::input::Key, map: &Map, objects: &mut Vec<Object>, messages: &mut Messages, inventory: &mut Vec<Object>, tcod: &mut Tcod) -> (PlayerAction, Option<Object>) {
    use tcod::input::Key;
    use tcod::input::KeyCode::*;
    use PlayerAction::*;

    let player_alive = objects[PLAYER].alive;

    match (key, player_alive) {
        (Key { code: Enter, alt: true, .. }, _) => {
            // Alt+Enter: toggle fullscreen
            let fullscreen = tcod.root.is_fullscreen();
            tcod.root.set_fullscreen(!fullscreen);
            (DidntTakeTurn, None)
        }
        (Key { code: Escape, .. }, _) => return (Exit, None),  // exit game
        (Key {printable: 'g', ..}, true) => {
            let item_id = objects.iter().position(|obj| {
                obj.pos() == objects[PLAYER].pos() && obj.item.is_some()
            });
            if let Some(item_id) = item_id {
                pick_item_up(item_id, objects, inventory, messages);
            }
            (DidntTakeTurn, None)
        }, 
        (Key {printable: 'i', ..}, true) => {
            let index = inventory_menu(inventory, "Selecione o item que desejar.\n", &mut tcod.root);
            if let Some(index) = index {
                use_item(index, inventory, objects, messages, tcod);
            }
            (TookTurn, None)
        },
        // movement keys
        (Key { code: Up, .. }, true) => {
            let ret = player_move_or_attack(0, -1, map, objects, messages);
            (TookTurn, ret)
        }
        (Key { code: Down, .. }, true) => {
            let ret = player_move_or_attack(0, 1, map, objects, messages);
            (TookTurn, ret)
        }
        (Key { code: Left, .. }, true) => {
            let ret = player_move_or_attack(-1, 0, map, objects, messages);
            (TookTurn, ret)
        }
        (Key { code: Right, .. }, true) => {
            let ret = player_move_or_attack(1, 0, map, objects, messages);
            (TookTurn, ret)
        }

        _ => (DidntTakeTurn, None),
    }
}

fn get_names_under_mouse(mouse: Mouse, objects: &[Object], fov_map: &FovMap) -> String {
    let (x, y) = (mouse.cx as i32, mouse.cy as i32);

    // create a list with the names of all objects at the mouse's coordinates and in FOV
    let names = objects
        .iter()
        .filter(|obj| {obj.pos() == (x, y) && fov_map.is_in_fov(obj.x, obj.y)})
        .map(|obj| obj.name.clone())
        .collect::<Vec<_>>();

    names.join(", ")  // join the names, separated by commas
}

fn place_objects(room: Rect, objects: &mut Vec<Object>, map: &Map) {
    // choose random number of monsters
    let num_monsters = rand::thread_rng().gen_range(0, MAX_ROOMS_MONSTERS + 1);

    for _ in 0..num_monsters {
        // choose random spot for this monster
        let x = rand::thread_rng().gen_range(room.x1 + 1, room.x2);
        let y = rand::thread_rng().gen_range(room.y1 + 1, room.y2);

        let mut monster = if rand::random::<f32>() < 0.8 {  // 80% chance of getting an orcorc
            // create an orc
            let mut orc = Object::new(x, y, 'o', "Orc".into() , colors::Color{
                r: 46,
                g: 204,
                b: 113
            }, true);
            orc.fighter = Some(Fighter{
                max_hp: 10, 
                hp: 10, 
                defense: 0,
                power: 3,
                on_death: DeathCallback::Monster
            });
            orc.ai = Some(Ai::Basic);
            orc
        } else {
            let mut troll = Object::new(x, y, 'T', "Troll".into() , colors::Color {
                r: 39,
                g: 174,
                b: 96
            }, true);
            troll.fighter = Some(Fighter {
                max_hp: 15, 
                hp: 15, 
                defense: 1, 
                power: 4,
                on_death: DeathCallback::Monster
            });
            troll.ai = Some(Ai::Basic);
            troll
        };
        monster.alive = true;
        objects.push(monster);
    }

    let num_items = rand::thread_rng().gen_range(0, MAX_ROOMS_ITEMS + 1);
    for _ in 0..num_items {
        let x = rand::thread_rng().gen_range(room.x1 + 1, room.x2);
        let y = rand::thread_rng().gen_range(room.y1 + 1, room.y2);

        // only place it if the tile is not blocked
        if !is_blocked(x, y, map, objects) {
            let dice = rand::random::<f32>();
            let item = if dice < 0.6 {
                let mut object = Object::new(x, y, '!', "Pocao de cura".to_string(), colors::Color {
                    r: 142,
                    g: 68,
                    b: 173
                }, false);
                object.item = Some(Item::new(ItemType::Heal, 5, 0));
                object
            } else if dice < 0.75 {
                let mut object = Object::new(x, y, 'º', "Bola de Fogo".to_string(), colors::LIGHT_RED, false);
                object.item = Some(Item::new(ItemType::FireBolt, 20, 5));
                object
            } else {
                if rand::random() {
                    let mut object = Object::new(x, y, '$', "Feitico de confusao".to_string(), colors::Color {
                        r: 211,
                        g: 84,
                        b: 0
                    }, false);
                    object.item = Some(Item::new(ItemType::Confuse, 0, 5));
                    object
                }
                else {
                    let mut object = Object::new(x, y, '*', "Feitico de medo".to_string(), colors::BLACK, false);
                    object.item = Some(Item::new(ItemType::Scare, 0, 5));
                    object
                }
               
            };
            
            objects.push(item);
        }

    }

}

fn is_blocked(x: i32, y: i32, map: &Map, objects: &[Object]) -> bool {
    // first test the map tile
    if map[x as usize][y as usize].blocked {
        return true;
    }
    // now check for any blocking objects
    objects.iter().any(|object| {
        object.blocks && object.pos() == (x, y)
    })
}

fn render_bar(panel: &mut Offscreen,
              x: i32,
              y: i32,
              total_width: i32,
              name: &str,
              value: i32,
              maximum: i32,
              bar_color: Color,
              back_color: Color)
{
    // render a bar (HP, experience, etc). First calculate the width of the bar
    let bar_width = (value as f32 / maximum as f32 * total_width as f32) as i32;

    // render the background first
    panel.set_default_background(back_color);
    panel.rect(x, y, total_width, 1, false, BackgroundFlag::Screen);

    // now render the bar on top
    panel.set_default_background(bar_color);
    if bar_width > 0 {
        panel.rect(x, y, bar_width, 1, false, BackgroundFlag::Screen);
    }

    panel.set_default_foreground(colors::WHITE);
    panel.print_ex(x + total_width / 2, y, BackgroundFlag::None, TextAlignment::Center, 
               &format!("{}: {}/{}", name, value, maximum));

    
}

fn message<T: Into<String>>(messages: &mut Messages, message: T, color: Color) {
    // if the buffer is full, remove the first message to make room for the new one
    if messages.len() == MSG_HEIGHT {
        messages.remove(0);
    }
    // add the new line as a tuple, with the text and the color
    messages.push((message.into(), color));

    
}

fn menu<T: AsRef<str>>(header: &str, options: &[T], width: i32,
                       root: &mut Root) -> Option<usize> {
    let header_height = root.get_height_rect(0,0, width, SCREEN_HEIGHT, header);
    let height = options.len() as i32 + header_height;

    let mut window = Offscreen::new(width, height); 
    window.set_default_background(colors::WHITE);
    window.print_rect_ex(0,0, width, height, BackgroundFlag::None, TextAlignment::Left, header);

    for (i, option_text) in options.iter().enumerate() {
        let menu_letter = (b'a' + i as u8) as char;
        let text = format!("({}) {}", menu_letter, option_text.as_ref());
        window.print_ex(0, header_height +  i as i32, BackgroundFlag::None, TextAlignment::Left, text);

    }

    // blit the contents of "window" to the root console
    let x = SCREEN_WIDTH / 2 - width / 2;
    let y = SCREEN_HEIGHT / 2 - height / 2;
    tcod::console::blit(&mut window, (0, 0), (width, height), root, (x, y), 1.0, 0.7);

    root.flush();

    let key = root.wait_for_keypress(true);

    if key.printable.is_alphabetic() {
        let i = key.printable.to_ascii_lowercase() as usize - 'a' as usize;
        if i < options.len() {
            Some(i)
        } else {
            None
        }
    }
    else {
        None
    }
    
}

fn inventory_menu(inventory: &Vec<Object>, header: &str, root: &mut Root) -> Option<usize> {
    // how a menu with each item of the inventory as an option
    let options = if inventory.len() == 0 {
        vec!["Inventory is empty.".into()]
    } else {
        inventory.iter().map(|item| { item.name.clone() }).collect()
    };

    let inventory_index = menu(header, &options, INVENTORY_WIDTH, root);

    // if an item was chosen, return it
    if inventory.len() > 0 {
        inventory_index
    } else {
        None
    }
}

fn closest_monster(max_range: i32, objects: &mut [Object], tcod: &Tcod) -> Option<usize> {
    let mut closest_enemy = None;
    let mut closest_dist = (max_range + 1) as f32;

    for(id, object) in objects.iter().enumerate() {
        if id != PLAYER && object.fighter.is_some() && object.ai.is_some() && tcod.fov.is_in_fov(object.x, object.y) {
            let dist = objects[PLAYER].distance_to(object);
            if dist < closest_dist {
                closest_enemy = Some(id);
                closest_dist = dist;
            }
        }
    }
    closest_enemy
}

fn use_item(inventory_id: usize, inventory: &mut Vec<Object>, objects: &mut [Object],
            messages: &mut Messages, tcod: &mut Tcod) {
    use ItemType::*;
    // just call the "use_function" if it is defined
    let object = inventory.iter().nth(inventory_id).expect("Error").clone();
    if let Some(item) = object.item {
        let on_use = match item.item_type {
            Heal => cast_heal,
            Damage => cast_damage,
            FireBolt => cast_fire_bolt,
            Confuse => cast_confuse,
            Scare => cast_scare,
            Merge => cast_merge
        };
        match on_use(inventory_id, objects, messages, item, tcod) {
            UseResult::UsedUp => {
                // destroy after use, unless it was cancelled for some reason
                inventory.remove(inventory_id);
            }
            UseResult::Cancelled => {
                message(messages, "Cancelled", colors::WHITE);
            }
        }
    } else {
        message(messages,
                format!("The {} cannot be used.", inventory[inventory_id].name),
                colors::WHITE);
    }
}

fn cast_scare(_inventory_id: usize, objects: &mut [Object], messages: &mut Messages, item: Item, tcod: &mut Tcod) -> UseResult {
    let monster_id = closest_monster(item.range as i32, objects, tcod);
    if let Some(monster_id) = monster_id {
        let old_ai = objects[monster_id].ai.take().unwrap_or(Ai::Basic);
        // replace the monster's AI with a "confused" one; after
        // some turns it will restore the old AI
        objects[monster_id].ai = Some(Ai::Scared {
            previous: Box::new(old_ai),
            turns: 5,
        });
        message(messages,
                format!("{} tem medo de voce e foge!",
                        objects[monster_id].name),
                colors::LIGHT_GREEN);
        UseResult::UsedUp
    } else {  // no enemy fonud within maximum range
        message(messages, "Nem um inimigo por perto.", colors::RED);
        UseResult::Cancelled
    }
}

fn cast_merge(_inventory_id: usize, objects: &mut [Object], messages: &mut Messages, item: Item, tcod: &mut Tcod) -> UseResult {
    let monster_id = closest_monster(item.range as i32, objects, tcod);
    let mut player = objects[PLAYER].clone();
    if let Some(monster_id) = monster_id {
        message(messages, format!("Voce se funde com {}.", objects[monster_id].name), colors::BLUE);
        player.merge(&mut objects[monster_id]);
        UseResult::UsedUp
    }
    else {
        UseResult::Cancelled
    }
}


fn cast_confuse(_inventory_id: usize, objects: &mut [Object], messages: &mut Messages, item: Item, tcod: &mut Tcod) -> UseResult {
    let monster_id = closest_monster(item.range as i32, objects, tcod);
    
    if let Some(monster_id) = monster_id {
        let old_ai = objects[monster_id].ai.take().unwrap_or(Ai::Basic);
        // replace the monster's AI with a "confused" one; after
        // some turns it will restore the old AI
        objects[monster_id].ai = Some(Ai::Confused {
            previous: Box::new(old_ai),
            turns: 5,
        });
        message(messages,
                format!("The eyes of {} look vacant, as he starts to stumble around!",
                        objects[monster_id].name),
                colors::LIGHT_GREEN);
        UseResult::UsedUp
    } else {  // no enemy fonud within maximum range
        message(messages, "No enemy is close enough to strike.", colors::RED);
        UseResult::Cancelled
    }
}


fn cast_fire_bolt(_inventory_id: usize, objects: &mut [Object], messages: &mut Messages, item: Item, tcod: &mut Tcod) -> UseResult {
    print!("Bola de fogo: {:?}", item);
    let monster_id = closest_monster(item.range as i32, objects, tcod);
    if let Some(monster_id) = monster_id {
        message(messages, format!("Uma bola de fogo atingiu o {}!\nO hit foi de {}", objects[monster_id].name, item.amount), colors::BLUE);
        objects[monster_id].take_damage(item.amount as i32, messages);
        UseResult::UsedUp
    }
    else {
        UseResult::Cancelled
    }
}


fn cast_damage(_inventory_id: usize, objects: &mut [Object], messages: &mut Messages,  item: Item, tcod: &mut Tcod) -> UseResult {
    // heal the player
    if let Some(fighter) = objects[PLAYER].fighter {
        
        if fighter.hp < 1 {
            message(messages, "Voce ja esta morto", colors::RED);
            return UseResult::Cancelled;
        }
        message(messages, "Voce foi ferido!", colors::LIGHT_VIOLET);
        objects[PLAYER].take_damage(item.amount as i32, messages);
        return UseResult::UsedUp;
        
    }
    UseResult::Cancelled
}

fn cast_heal(_inventory_id: usize, objects: &mut [Object], messages: &mut Messages,  item: Item, tcod: &mut Tcod) -> UseResult {
    // heal the player
    if let Some(fighter) = objects[PLAYER].fighter {
        
        if fighter.hp == fighter.max_hp {
            message(messages, "You are already at full health.", colors::RED);
            return UseResult::Cancelled;
        }
        message(messages, "Voce se sente melhor!", colors::LIGHT_VIOLET);
        objects[PLAYER].heal(item.amount as i32);
        return UseResult::UsedUp;
        
    }
    UseResult::Cancelled
}

fn handle_camera(camera: &mut (i32, i32), objects: &mut [Object]) {
    let player = objects[PLAYER].clone();
    if player.x -camera.0 < -1 {
        camera.0 -= 1
    } else if player.x - camera.0 > 0 {
       camera.0 += 1
    }
    if player.y - camera.1 < -1 {
        camera.1 -= 1
    } else if player.y - camera.1 > 0 {
        camera.1 += 1
    }
}

fn main() {

    let mut key = Default::default();
    let mut root = Root::initializer()
        .font("bluebox.png", FontLayout::AsciiInRow)
        .font_type(FontType::Greyscale)
        .size(SCREEN_WIDTH, SCREEN_HEIGHT)
        .title("Rust/libtcod tutorial")
        .init();

    tcod::system::set_fps(LIMIT_FPS); 

    let mut tcod = Tcod {
        root: root,
        con:  Offscreen::new(MAP_WIDTH, MAP_HEIGHT),
        panel: Offscreen::new(SCREEN_WIDTH, PANEL_HEIGHT),
        fov:  FovMap::new(MAP_WIDTH, MAP_HEIGHT),
        mouse: Default::default(),
    };

    let mut player = Object::new(0, 0, '@', "Player".into(), colors::WHITE, true);
    player.alive = true;
    player.fighter = Some(Fighter {
        max_hp: 30,
        hp: 30, 
        defense: 2,
        power: 5,
        on_death: DeathCallback::Player
    });

    


    let mut inventory = vec![];
    // the list of objects with those two
    let mut objects = vec![player];

    let mut map = make_map(&mut objects);

    let mut player = objects[PLAYER].clone();

    let mut camera: (i32, i32) = (player.x, player.y);

    //let mut inventory = Vec<Object>[];

    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            tcod.fov.set(x, y,
                        !map[x as usize][y as usize].block_sight,
                        !map[x as usize][y as usize].blocked);
        }
    }

    let mut messages = vec![];

    message(&mut messages, "Bem vindo!", colors::RED);

    // let t = thread::spawn(move || {
    //     if objects[PLAYER].alive {
    //         for id in 0..objects.len() {
    //             if objects[id].ai.is_some() {
    //                 ai_take_turn(id, &map, &mut objects, &fov_map);
    //             } 
    //         }
    //     }
    //     thread::sleep(time::Duration::from_secs(1));
    // });
    // t.join().unwrap();

    render_all(&mut tcod, &mut objects, &mut map, true, &messages, &mut camera);
    
    while !tcod.root.window_closed() {
       
        if let Some(fighter) = objects[PLAYER].fighter {
            tcod.root.print_ex(1, SCREEN_HEIGHT - 2, BackgroundFlag::None, TextAlignment::Left,
                        format!("HP: {}/{} ", fighter.hp, fighter.max_hp));
        }

        match input::check_for_event(input::MOUSE | input::KEY_PRESS) {
            Some((_, Event::Mouse(m))) => {
                tcod.mouse = m;
                key = Default::default();
            },
            Some((_, Event::Key(k))) => {
                key = k;
                tcod.mouse = Default::default();
            },
            _ => {
                key = Default::default();
            },
        }


       

        for object in &objects {
            object.clear(&mut tcod.con)
        }

        let (player_action, option) = handle_keys(key, &map, &mut objects, &mut messages, &mut inventory, &mut tcod);
        if option.is_some() {
            objects[PLAYER] = option.unwrap();
        }
        if player_action == PlayerAction::Exit {
            break
        } else if player_action == PlayerAction::TookTurn {
            handle_camera(&mut camera, &mut objects);
        }

        if objects[PLAYER].alive && player_action != PlayerAction::DidntTakeTurn {
            for id in 0..objects.len() {
                if objects[id].ai.is_some() {
                    ai_take_turn(id, &map, &mut objects, &mut tcod.fov, &mut messages)
                }
            }
        }

        render_all(&mut tcod, &mut objects, &mut map, key != Default::default(), &messages, &mut camera);

        tcod.root.flush();
    }
}