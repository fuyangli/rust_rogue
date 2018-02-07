extern crate tcod;
extern crate rand;

use std::cmp;
use std::thread;
use std::time;


use tcod::console::*;
use tcod::colors::{self, Color};
use tcod::map::{Map as FovMap, FovAlgorithm};
use rand::Rng;

use tcod::input::{self, Event, Mouse};

const PLAYER: usize = 0;

// actual size of the window
const SCREEN_WIDTH: i32 = 80;
const SCREEN_HEIGHT: i32 = 50;

// size of the map
const MAP_WIDTH: i32 = 80;
const MAP_HEIGHT: i32 = 43;

// Panel
const BAR_WIDTH: i32 = 20;
const PANEL_HEIGHT: i32 = 7;
const PANEL_Y: i32 = SCREEN_HEIGHT - PANEL_HEIGHT;

// Mensagens
const MSG_X: i32 = BAR_WIDTH + 2;
const MSG_WIDTH: i32 = SCREEN_WIDTH - BAR_WIDTH - 2;
const MSG_HEIGHT: usize = PANEL_HEIGHT as usize - 1;

//parameters for dungeon generator
const ROOM_MAX_SIZE: i32 = 15;
const ROOM_MIN_SIZE: i32 = 5;
const MAX_ROOMS: i32 = 30;
const MAX_ROOMS_MONSTERS: i32 = 3;

const MAX_ROOMS_ITEMS: i32 = 3;

const FOV_ALGO: FovAlgorithm = FovAlgorithm::Basic;  // default FOV algorithm
const FOV_LIGHT_WALLS: bool = true;  // light walls or not
const TORCH_RADIUS: i32 = 10;

const LIMIT_FPS: i32 = 20;  // 20 frames-per-second maximum

const COLOR_DARK_WALL: Color = Color { r: 0, g: 0, b: 100 };
const COLOR_LIGHT_WALL: Color = Color { r: 130, g: 110, b: 50 };
const COLOR_DARK_GROUND: Color = Color { r: 50, g: 50, b: 150 };
const COLOR_LIGHT_GROUND: Color = Color { r: 200, g: 180, b: 50 };

type Map = Vec<Vec<Tile>>;
type Messages = Vec<(String, Color)>;




#[derive(Clone, Copy, Debug, PartialEq)]
struct Fighter {
    max_hp: i32,
    hp: i32,
    defense: i32,
    power: i32,
    on_death: DeathCallback,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DeathCallback {
    Player,
    Monster,
}

impl DeathCallback {
    fn callback(self, object: &mut Object) {
        use DeathCallback::*;
        let callback: fn(&mut Object) = match self {
            Player => player_death,
            Monster => monster_death,
        };
        callback(object);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Ai;

/// A tile of the map and its properties
#[derive(Clone, Copy, Debug)]
struct Tile {
    blocked: bool,
    explored: bool,
    block_sight: bool,
    char: char
}

impl Tile {
    pub fn new(blocked: bool, explored: bool, block_sight: bool, char: char) -> Self {
        Tile {blocked: blocked, explored: blocked, block_sight: block_sight, char: char}
    }
    pub fn empty() -> Self {
        Tile{blocked: false, explored: false, block_sight: false, char: ' '}
    }

    pub fn wall() -> Self {
        Tile{blocked: true, explored: false, block_sight: true, char: ' '}
    }
}

/// A rectangle on the map, used to characterise a room.
#[derive(Clone, Copy, Debug)]
struct Rect {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    connections: i32,
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

fn player_death(player: &mut Object) {
    // the game ended!
    println!("You died!");

    // for added effect, transform the player into a corpse!
    player.char = '%';
    player.color = colors::DARK_RED;
}

fn monster_death(monster: &mut Object) {
    // transform it into a nasty corpse! it doesn't block, can't be
    // attacked and doesn't move
    println!("{} is dead!", monster.name);
    monster.char = 'x';
    monster.color = colors::DARK_RED;
    monster.blocks = false;
    monster.fighter = None;
    monster.ai = None;
    monster.name = format!("remains of {}", monster.name);
}

#[derive(Debug)]
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
            ai: None
        }
    }

    /// move by the given amount, if the destination is not blocked
    pub fn move_by(&mut self, dx: i32, dy: i32, map: &Map) {
        if !map[(self.x + dx) as usize][(self.y + dy) as usize].blocked {
            self.x += dx;
            self.y += dy;
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

    pub fn take_damage(&mut self, damage: i32) {
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
                fighter.on_death.callback(self);
            }
        }
    }

    pub fn attack(&mut self, target: &mut Object) {
    // a simple formula for attack damage
        let damage = self.fighter.map_or(0, |f| f.power) - target.fighter.map_or(0, |f| f.defense);
        if damage > 0 {
            // make the target take some damage
            println!("{} attacks {} for {} hit points.", self.name, target.name, damage);
            target.take_damage(damage);
        } else {
            println!("{} attacks {} but it has no effect!", self.name, target.name);
        }
    }

    pub fn merge(&mut self, target: &mut Object) -> Object {
        let defense = ( ((self.fighter.map_or(0, |f| f.defense) + target.fighter.map_or(0, |f| f.defense)) as f32) * 0.75) as i32;
        let power = (((self.fighter.map_or(0, |f| f.power) + target.fighter.map_or(0, |f| f.power)) as f32) * 0.75)  as i32;
        let max_hp = (((self.fighter.map_or(0, |f| f.max_hp) + target.fighter.map_or(0, |f| f.max_hp)) as f32) * 0.75)  as i32; 
        let hp = (((self.fighter.map_or(0, |f| f.hp) + target.fighter.map_or(0, |f| f.hp)) as f32) * 0.75) as i32;

        println!("max_hp: {}", max_hp);

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

        object.ai = self.ai;
        object.alive = true;
        
        object
    }

    // TODO: Implementar método de fusão
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

fn ai_take_turn(monster_id: usize, map: &Map, objects: &mut [Object], fov_map: &FovMap) {
    // a basic monster takes its turn. If you can see it, it can see you
    let (monster_x, monster_y) = objects[monster_id].pos();
    if fov_map.is_in_fov(monster_x, monster_y) {
        if objects[monster_id].distance_to(&objects[PLAYER]) >= 2.0 {
            // move towards player if far away
            let (player_x, player_y) = objects[PLAYER].pos();
            //move_towards(monster_id, player_x, player_y, map, objects);
            move_away(monster_id, player_x, player_y, map, objects);
        } else if objects[PLAYER].fighter.map_or(false, |f| f.hp > 0) {
            // close enough, attack! (if the player is still alive.)
            let (monster, player) = mut_two(monster_id, PLAYER, objects);
            monster.attack(player);
        }
    }
}

fn move_by(id: usize, dx: i32, dy: i32, map: &Map, objects: &mut [Object]) {
    let (x, y) = objects[id].pos();
    if !is_blocked(x + dx, y + dy, map, objects) {
        objects[id].set_pos(x + dx, y + dy);
    }
}



fn create_room(room: Rect, map: &mut Map, char: char) {
    // go through the tiles in the rectangle and make them passable
    for x in (room.x1 + 1)..room.x2 {
        for y in (room.y1 + 1)..room.y2 {
            map[x as usize][y as usize] = Tile::empty();
        }
    }
    map[room.x2 as usize][room.y2 as usize] = Tile::new(false, false, true, char);
}

fn create_h_tunnel(x1: i32, x2: i32, y: i32, map: &mut Map) {
    for x in cmp::min(x1, x2)..(cmp::max(x1, x2) + 1) {
        map[x as usize][y as usize] = Tile::empty();
    }
}

fn create_v_tunnel(y1: i32, y2: i32, x: i32, map: &mut Map) {
    for y in cmp::min(y1, y2)..(cmp::max(y1, y2) + 1) {
        map[x as usize][y as usize] = Tile::empty();
    }
}

fn make_map(objects: &mut Vec<Object>) -> (Map) {

    let mut map = vec![vec![Tile::wall(); MAP_HEIGHT as usize]; MAP_WIDTH as usize];

    let mut rooms = vec![];

    for i in 0..MAX_ROOMS {
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
            create_room(new_room, &mut map, (i as u8) as char);

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
    rooms.sort_by(|a, b| {
        let (a_x, a_y) = a.center();
        let (b_x, b_y) = b.center();
        if a_y < b_y {
            return std::cmp::Ordering::Less;
        }
        if a_y == b_y {
            if a_x == b_x {
                return std::cmp::Ordering::Equal;
            }
            if a_x < b_x {
                return std::cmp::Ordering::Less;
            }
        }
        return std::cmp::Ordering::Greater;
    });
    println!("{:?}", rooms.len());

    for i in 0..rooms.len() {

        if i > (rooms.len() - 2) {
            println!("{:?}", i);
            continue;
        }
        let mut new_room = rooms[i as usize + 1];
        let (new_x, new_y) = new_room.center();
        let mut previous_room = rooms[i as usize];
        let (prev_x, prev_y) = previous_room.center();

        if previous_room.connections < 2 {
            // toss a coin (random bool value -- either true or false)
            if rand::random() {
                // first move horizontally, then vertically
                create_h_tunnel(prev_x, new_x, prev_y, &mut map);
                create_v_tunnel(prev_y, new_y, new_x, &mut map);
            } else {
                // first move vertically, then horizontally
                create_v_tunnel(prev_y, new_y, prev_x, &mut map);
                create_h_tunnel(prev_x, new_x, new_y, &mut map);
            }
            previous_room.connections += 1;
            new_room.connections += 1;
        }
        // println!("Previous: {:?}", previous_room);
        // println!("New: {:?}", new_room);
    }

    (map)
}

fn render_all(root: &mut Root, con: &mut Offscreen, objects: &[Object], map: &mut Map,
              fov_map: &mut FovMap, fov_recompute: bool, panel: &mut Offscreen, messages: &Messages, mouse: Mouse) {
    if fov_recompute {
        // recompute FOV if needed (the player moved or something)
        let player = &objects[PLAYER];
        fov_map.compute_fov(player.x, player.y, TORCH_RADIUS, FOV_LIGHT_WALLS, FOV_ALGO);

        // go through all tiles, and set their background color
        for y in 0..MAP_HEIGHT {
            for x in 0..MAP_WIDTH {
                let tile = &mut map[x as usize][y as usize];
                let visible = fov_map.is_in_fov(x, y);
                let wall = tile.block_sight;
                let color = match (visible, wall) {
                    // outside of field of view:
                    (false, true) => COLOR_DARK_WALL,
                    (false, false) => COLOR_DARK_GROUND,
                    // inside fov:
                    (true, true) => COLOR_LIGHT_WALL,
                    (true, false) => COLOR_LIGHT_GROUND,
                };

                let mut explored = tile.explored;
                if visible {
                    // since it's visible, explore it
                    explored = true;
                }
                if explored {
                    // show explored tiles only (any visible tile is explored already)
                    con.set_char_background(x, y, color, BackgroundFlag::Set);
                    if  (tile).char != ' ' {
                        con.set_default_foreground(colors::WHITE);
                        con.put_char(x, y, (tile).char, BackgroundFlag::None);
                    }
                                            
                }
            }
        }

    }

    let mut to_draw : Vec<_> = objects.iter().filter(|o| fov_map.is_in_fov(o.x, o.y)).collect();
    to_draw.sort_by(|o1, o2| {
        o1.blocks.cmp(&o2.blocks)
    });
    for object in &to_draw {
        if fov_map.is_in_fov(object.x, object.y) {
            object.draw(con);
        }
    }

  
    blit(con, (0, 0), (MAP_WIDTH, MAP_HEIGHT), root, (0, 0), 1.0, 1.0);

    panel.set_default_background(colors::BLACK);
    panel.clear();

    let mut y = MSG_HEIGHT as i32;
    for &(ref msg, color) in messages.iter().rev() {
        let msg_height = panel.get_height_rect(MSG_X, y, MSG_WIDTH, 0, msg);
        y -= msg_height;
        if y < 0 {
            break;
        }
        panel.set_default_foreground(color);
        panel.print_rect(MSG_X, y, MSG_WIDTH, 0, msg);
    }

    // show the player's stats
    let hp = objects[PLAYER].fighter.map_or(0, |f| f.hp);
    let max_hp = objects[PLAYER].fighter.map_or(0, |f| f.max_hp);
    render_bar(panel, 1, 1, BAR_WIDTH, "HP", hp, max_hp, colors::LIGHT_RED, colors::DARKER_RED);

    panel.set_default_foreground(colors::LIGHT_GREY);
    panel.print_ex(1, 0, BackgroundFlag::None, TextAlignment::Left, get_names_under_mouse(mouse, objects, fov_map));

    blit(panel, (0, 0), (MAP_WIDTH, PANEL_HEIGHT), root, (0, PANEL_Y), 1.0, 1.0);

    

    
    
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

fn player_move_or_attack(dx: i32, dy: i32, map: &Map, objects: &mut [Object]) -> Option<Object> {
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
            let new_player = player.merge(target);
            return Some(new_player);
        }
        None => {
            move_by(PLAYER, dx, dy, map, objects);
            return None;
        }
    }
    
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PlayerAction {
    TookTurn,
    DidntTakeTurn,
    Exit,
}

fn handle_keys(key: tcod::input::Key, root: &mut Root, map: &Map, objects: &mut [Object], delta_time_in_nano: u32) -> (PlayerAction, Option<Object>) {
    use tcod::input::Key;
    use tcod::input::KeyCode::*;
    use PlayerAction::*;

    let player_alive = objects[PLAYER].alive;

    match (key, player_alive) {
        (Key { code: Enter, alt: true, .. }, _) => {
            // Alt+Enter: toggle fullscreen
            let fullscreen = root.is_fullscreen();
            root.set_fullscreen(!fullscreen);
            (DidntTakeTurn, None)
        }
        (Key { code: Escape, .. }, _) => return (Exit, None),  // exit game

        // movement keys
        (Key { code: Up, .. }, true) => {
            let ret = player_move_or_attack(0, -1, map, objects);
            (TookTurn, ret)
        }
        (Key { code: Down, .. }, true) => {
            let ret = player_move_or_attack(0, 1, map, objects);
            (TookTurn, ret)
        }
        (Key { code: Left, .. }, true) => {
            let ret = player_move_or_attack(-1, 0, map, objects);
            (TookTurn, ret)
        }
        (Key { code: Right, .. }, true) => {
            let ret = player_move_or_attack(1, 0, map, objects);
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

        let mut monster = if rand::random::<f32>() < 0.8 {  // 80% chance of getting an orc
            // create an orc
            let mut orc = Object::new(x, y, 'o', "Orc".into() , colors::DESATURATED_GREEN, true);
            orc.fighter = Some(Fighter{
                max_hp: 10, 
                hp: 10, 
                defense: 0,
                power: 3,
                on_death: DeathCallback::Monster
            });
            orc.ai = Some(Ai);
            orc
        } else {
            let mut troll = Object::new(x, y, 'T', "Troll".into() , colors::DARKER_GREEN, true);
            troll.fighter = Some(Fighter {
                max_hp: 15, 
                hp: 15, 
                defense: 1, 
                power: 4,
                on_death: DeathCallback::Monster
            });
            troll.ai = Some(Ai);
            troll
        };
        monster.alive = true;
        objects.push(monster);
    }

    // let num_items = rand::thread_rng().gen_range(0, MAX_ROOMS_ITEMS + 1);
    // for _ in 0..num_items {
    //     let x = rand::thread_rng().gen_range(room.x1 + 1, room.x2);
    //     let y = rand::thread_rng().gen_range(room.y1 + 1, room.y2);

    //     // only place it if the tile is not blocked
    //     if !is_blocked(x, y, map, objects) {
    //         let mut object = Object::new(x, y, '!', "healing potion", colors::VIOLET, false);
    //         object.fighter = None;
    //         objects.push(object);
    //     }
    // }
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

fn main() {


    let mut mouse = Default::default();
    let mut key = Default::default();


    let mut root = Root::initializer()
        .font("arial10x10.png", FontLayout::Tcod)
        .font_type(FontType::Greyscale)
        .size(SCREEN_WIDTH, SCREEN_HEIGHT)
        .title("Rust/libtcod tutorial")
        .init();
    tcod::system::set_fps(LIMIT_FPS); 
    let mut con = Offscreen::new(MAP_WIDTH, MAP_HEIGHT);

    let mut panel = Offscreen::new(SCREEN_WIDTH, PANEL_HEIGHT);

    let mut player = Object::new(0, 0, '@', "Player".into(), colors::WHITE, true);
    player.alive = true;
    player.fighter = Some(Fighter {
        max_hp: 30,
        hp: 30, 
        defense: 2,
        power: 5,
        on_death: DeathCallback::Player
    });

    // the list of objects with those two
    let mut objects = vec![player];

    let mut map = make_map(&mut objects);

    //let mut inventory = Vec<Object>[];

    // create the FOV map, according to the generated map
    let mut fov_map = FovMap::new(MAP_WIDTH, MAP_HEIGHT);
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            fov_map.set(x, y,
                        !map[x as usize][y as usize].block_sight,
                        !map[x as usize][y as usize].blocked);
        }
    }

    // force FOV "recompute" first time through the game loop
    let mut previous_player_position = (-1, -1);

    let mut messages = vec![];

    message(&mut messages, "Welcome stranger! Prepare to perish in the Tombs of the Ancient Kings.",
        colors::RED);

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

    let mut old_time = std::time::SystemTime::now();
   
    while !root.window_closed() {

        let mut now_time = std::time::SystemTime::now();
        let delta_time = now_time.duration_since(old_time);
        old_time = now_time;

        let delta_time_in_nano = delta_time.unwrap().subsec_nanos();
        
        if let Some(fighter) = objects[PLAYER].fighter {
            root.print_ex(1, SCREEN_HEIGHT - 2, BackgroundFlag::None, TextAlignment::Left,
                        format!("HP: {}/{} ", fighter.hp, fighter.max_hp));
        }

        match input::check_for_event(input::MOUSE | input::KEY_PRESS) {
            Some((_, Event::Mouse(m))) => {
                mouse = m;
                key = Default::default();
            },
            Some((_, Event::Key(k))) => {
                key = k;
                mouse = Default::default();
            },
            _ => key = Default::default(),
        }

        // render the screen
        let fov_recompute = previous_player_position != (objects[0].x, objects[0].y);
        render_all(&mut root, &mut con, &mut objects, &mut map, &mut fov_map, fov_recompute, &mut panel, &messages, mouse);

        root.flush();

        // erase all objects at their old locations, before they move
        for object in &objects {
            object.clear(&mut con)
        }

        // handle keys and exit game if needed
        let player_pos = objects[PLAYER].pos();
        let (player_action, option) = handle_keys(key, &mut root, &map, &mut objects, delta_time_in_nano);
        if(option.is_some()) {
            objects[PLAYER] = option.unwrap();
        }
        if player_action == PlayerAction::Exit {
            break
        }

        if objects[PLAYER].alive && player_action != PlayerAction::DidntTakeTurn {
            for id in 0..objects.len() {
                if objects[id].ai.is_some() {
                    ai_take_turn(id, &map, &mut objects, &fov_map);
                }
            }
        }
    }
}