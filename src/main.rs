use rand::Rng;
use sdl2::event::Event;
use sdl2::keyboard::Scancode;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::WindowCanvas;
use std::fs;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant};
use std::{env, thread};
// use sdl2::audio::{AudioCallback, AudioSpecDesired};

// fn display(display_data: &[[bool; 64]; 32]) {
//     display_data.iter().for_each(|i| {
//         i.iter().for_each(|j| {
//             print!("{}", if *j { "██" } else { "  " });
//         });
//         print!("\n")
//     });
// }

fn display_canvas(display_data: &[[bool; 64]; 32], scale: u32, canvas: &mut WindowCanvas) {
    canvas.set_draw_color(Color::RGB(0, 0, 0));
    canvas.clear();
    canvas.set_draw_color(Color::RGB(255, 255, 255));
    for (y, row) in display_data.iter().enumerate() {
        for (x, elem) in row.iter().enumerate() {
            if *elem {
                let rect = Rect::new(
                    (x as u32 * scale) as i32,
                    (y as u32 * scale) as i32,
                    scale,
                    scale,
                );
                canvas.fill_rect(rect).unwrap();
            }
        }
    }
    canvas.present();
}

fn map_scancode_to_chip8(scancode: Scancode) -> Option<u8> {
    match scancode {
        Scancode::Num1 => Some(0x1),
        Scancode::Num2 => Some(0x2),
        Scancode::Num3 => Some(0x3),
        Scancode::Num4 => Some(0xC),
        Scancode::Q => Some(0x4),
        Scancode::W => Some(0x5),
        Scancode::E => Some(0x6),
        Scancode::R => Some(0xD),
        Scancode::A => Some(0x7),
        Scancode::S => Some(0x8),
        Scancode::D => Some(0x9),
        Scancode::F => Some(0xE),
        Scancode::Z => Some(0xA),
        Scancode::X => Some(0x0),
        Scancode::C => Some(0xB),
        Scancode::V => Some(0xF),
        _ => None,
    }
}

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    println!("{:?}", args);
    struct Quirks {
        shift: bool,
        memory_increment_by_x: bool,
        memory_leave_unchanged: bool,
        wrap: bool,
        jump: bool,
        // vblank:bool, // not implemented
        logic: bool,
    }

    let original_chip_8 = Quirks {
        shift: false,
        memory_increment_by_x: false,
        memory_leave_unchanged: false,
        wrap: false,
        jump: false,
        // vblank: false,
        logic: true,
    };

    // Scrolling instruction not implemented
    // let legacy_superchip = Quirks{
    //     shift:true,
    //     memory_increment_by_x:false,
    //     memory_leave_unchanged: true,
    //     wrap: false,
    //     jump: true,
    //     vblank: false,
    //     logic: false,
    // };

    let current_quirks = original_chip_8;

    const VIDEO_WIDTH: usize = 64;
    const VIDEO_HEIGHT: usize = 32;
    const VIDEO_SCALE: u32 = 10;
    const CYCLE_SPEED: usize = 500;
    const PERIOD: f64 = 1.0 / (CYCLE_SPEED as f64);

    const FONT_SET_START_ADDRESS: usize = 0x050;

    let sdl_context = sdl2::init()?;
    let video = sdl_context.video()?;
    // let audio_subsystem = sdl_context.audio()?;
    let window = video
        .window(
            "CHIP-8 Emulator",
            (VIDEO_WIDTH as u32) * VIDEO_SCALE,
            (VIDEO_HEIGHT as u32) * VIDEO_SCALE,
        )
        .position_centered()
        .build()
        .map_err(|e| e.to_string())?;
    let mut canvas = window.into_canvas().build().map_err(|e| e.to_string())?;
    let mut event_pump = sdl_context.event_pump()?;
    let mut chip8_keys = [false; 16]; // CHIP-8 key state

    // 0x000-0x1FF - Chip 8 interpreter (contains font set in emu)
    // 0x050-0x0A0 - Used for the built in 4x5 pixel font set (0-F)
    // 0x200-0xFFF - Program ROM and work RAM
    let mut memory: [u8; 4096] = [0; 4096];
    let mut pc: u16 = 0x200;
    let mut registers: [u8; 16] = [0; 16];
    let mut I: u16 = 0;
    let mut stack: Vec<u16> = Vec::new();

    static FONT_SET: [u8; 80] = [
        0xF0, 0x90, 0x90, 0x90, 0xF0, // 0
        0x20, 0x60, 0x20, 0x20, 0x70, // 1
        0xF0, 0x10, 0xF0, 0x80, 0xF0, // 2
        0xF0, 0x10, 0xF0, 0x10, 0xF0, // 3
        0x90, 0x90, 0xF0, 0x10, 0x10, // 4
        0xF0, 0x80, 0xF0, 0x10, 0xF0, // 5
        0xF0, 0x80, 0xF0, 0x90, 0xF0, // 6
        0xF0, 0x10, 0x20, 0x40, 0x40, // 7
        0xF0, 0x90, 0xF0, 0x90, 0xF0, // 8
        0xF0, 0x90, 0xF0, 0x10, 0xF0, // 9
        0xF0, 0x90, 0xF0, 0x90, 0x90, // A
        0xE0, 0x90, 0xE0, 0x90, 0xE0, // B
        0xF0, 0x80, 0x80, 0x80, 0xF0, // C
        0xE0, 0x90, 0x90, 0x90, 0xE0, // D
        0xF0, 0x80, 0xF0, 0x80, 0xF0, // E
        0xF0, 0x80, 0xF0, 0x80, 0x80, // F
    ];
    // load fonts
    for n in 0..FONT_SET.len() {
        memory[FONT_SET_START_ADDRESS + n] = FONT_SET[n];
    }

    let mut display_data: [[bool; 64]; 32] = [[false; 64]; 32];
    let path = match env::args().nth(1) {
        Some(path) => path,
        None => return Err("No path specified".to_string()),
    };

    let data = fs::read(path).expect("Unable to read file");
    for n in 0..data.len() {
        memory[0x200 + n] = data[n];
    }
    // for n in 0..(data.len())/2 {
    //     let num = u16::from_be_bytes([data[2*n],data[(2*n)+1]]);
    //     print!("{:X}{:X} {:04X}\n", data[2*n],data[(2*n)+1],num);
    // }

    let delay_timer = Arc::new(AtomicU8::new(0));
    let timer_clone = Arc::clone(&delay_timer);

    let sound_timer = Arc::new(AtomicU8::new(0));
    let sound_timer_clone = Arc::clone(&sound_timer);

    thread::spawn(move || {
        loop {
            let current = timer_clone.load(Ordering::Relaxed);
            let sound = sound_timer_clone.load(Ordering::Relaxed);
            if current > 0 {
                timer_clone.store(current - 1, Ordering::Relaxed);
            }
            if sound > 0 {
                sound_timer_clone.store(sound - 1, Ordering::Relaxed);
                print!("beep {}", '\x07');
                std::io::stdout().flush().unwrap();
            } else {
            }
            thread::sleep(Duration::from_millis(1000 / 60)); // 60 Hz
        }
    });

    'main_loop: loop {
        let start_time = Instant::now();
        // fetch
        let instruction: u16 = u16::from_be_bytes([memory[pc as usize], memory[(pc + 1) as usize]]);
        // println!("{:04X?}", instruction);
        std::io::stdout().flush().unwrap();

        pc += 2;
        let i1: usize = (instruction >> 12) as usize;
        let i2: usize = ((instruction & 0x0F00) >> 8) as usize;
        let i3: usize = ((instruction & 0x00F0) >> 4) as usize;
        let i4: usize = ((instruction & 0x000F) >> 0) as usize;
        let nn: usize = (instruction & 0x00FF) as usize;
        let nnn: usize = (instruction & 0x0FFF) as usize;

        // decode
        match i1 {
            0 => {
                if i2 == 0 && i3 == 0xE && i4 == 0x0 {
                    display_data
                        .iter_mut()
                        .for_each(|row| row.iter_mut().for_each(|l| *l = false));
                    display_canvas(&display_data, VIDEO_SCALE, &mut canvas);
                } else if i2 == 0 && i3 == 0xE && i4 == 0xE {
                    pc = stack.pop().unwrap_or_else(|| 0)
                }
            }
            1 => {
                // let address: u16 = instruction & 0x0FFF;
                pc = nnn as u16;
            }
            2 => {
                // let address: u16 = instruction & 0x0FFF;
                stack.push(pc);
                pc = nnn as u16;
            }
            3 => {
                let byte: u8 = (instruction & 0x00FF) as u8;
                if registers[i2] == byte {
                    pc += 2;
                }
            }
            4 => {
                let byte: u8 = (instruction & 0x00FF) as u8;
                if registers[i2] != byte {
                    pc += 2;
                }
            }
            5 => {
                if i4 == 0 && registers[i2] == registers[i3] {
                    pc += 2
                }
            }
            6 => {
                let byte: u8 = (instruction & 0x00FF) as u8;
                registers[i2] = byte;
            }
            7 => {
                let byte: u8 = (instruction & 0x00FF) as u8;
                registers[i2] = registers[i2].wrapping_add(byte);
            }
            8 => {
                match i4 {
                    0 => registers[i2] = registers[i3],
                    1 => {
                        registers[i2] |= registers[i3];
                        if current_quirks.logic {
                            registers[0xF] = 0;
                        }
                    }
                    2 => {
                        registers[i2] &= registers[i3];
                        if current_quirks.logic {
                            registers[0xF] = 0;
                        }
                    }
                    3 => {
                        registers[i2] ^= registers[i3];
                        if current_quirks.logic {
                            registers[0xF] = 0;
                        }
                    }
                    4 => {
                        let mut sum: u16 = (registers[i2] as u16) + (registers[i3] as u16);
                        if sum > 0xFF {
                            sum = sum - 0x100;
                            registers[i2] = sum as u8;
                            registers[0xF] = 1;
                        } else {
                            registers[i2] = sum as u8;
                            registers[0xF] = 0;
                        }
                    }
                    5 => {
                        // flag
                        let temp = registers[i2];
                        registers[i2] = registers[i2].wrapping_sub(registers[i3]);
                        // if (temp == 20 && registers[i3]==15) {
                        //     print!("Subtract 20-15 result {}", temp);
                        // }
                        if temp >= registers[i3] {
                            registers[0xF] = 1;
                        } else {
                            registers[0xF] = 0;
                        }
                    }
                    6 => {
                        let temp = registers[i2];
                        if current_quirks.shift {
                            registers[i2] >>= 1;
                            registers[0xF] = temp & 0x1;
                        } else {
                            registers[i2] = registers[i3] >> 1;
                            registers[0xF] = registers[i3] & 0x1;
                        }
                    }
                    7 => {
                        // flag
                        let temp = registers[i2];
                        registers[i2] = registers[i3].wrapping_sub(registers[i2]);
                        if registers[i3] >= temp {
                            registers[0xF] = 1;
                        } else {
                            registers[0xF] = 0;
                        }
                    }
                    0xE => {
                        let temp = registers[i2];
                        if current_quirks.shift {
                            registers[i2] <<= 1;
                            registers[0xF] = (temp & 0x80) >> 7;
                        } else {
                            registers[i2] = registers[i3] << 1;
                            registers[0xF] = (registers[i3] & 0x80) >> 7;
                        }
                    }
                    _ => {}
                }
            }
            9 => {
                if i4 == 0 {
                    if registers[i2] != registers[i3] {
                        pc += 2;
                    }
                }
            }
            0xA => {
                // let address: u16 = instruction & 0x0FFF;
                I = nnn as u16;
            }
            0xB => {
                // let address: u16 = instruction & 0x0FFF;
                if current_quirks.jump {
                    pc = (registers[i2] as u16) + nnn as u16;
                } else {
                    pc = (registers[0] as u16) + nnn as u16;
                }
            }
            0xC => {
                let byte: u8 = (instruction & 0x00FF) as u8;
                let mut rng = rand::rng();
                let rand_byte: u8 = rng.random();
                registers[i2] = rand_byte & byte;
            }
            0xD => {
                let x_pos = registers[i2] % (VIDEO_WIDTH as u8);
                let y_pos = registers[i3] % (VIDEO_HEIGHT as u8);
                println!("printing sprite {} {}", x_pos, y_pos);
                let height: u8 = i4 as u8;
                registers[0xF] = 0;
                for row in 0..height {
                    let sprite_byte: u8 = memory[(I + row as u16) as usize];
                    for col in 0..8 {
                        let sprite_pixel: u8 = ((sprite_byte) & (0b10000000 >> col)) >> (7 - col);
                        let is_sprite_pixel_on: bool =
                            if sprite_pixel == 0x01 { true } else { false };

                        let mut clipped: bool = false;
                        let mut final_x_pos: usize = (x_pos + col) as usize;
                        if final_x_pos < 0 as usize || final_x_pos >= VIDEO_WIDTH {
                            clipped = true;
                        }

                        let mut final_y_pos: usize = (y_pos + row) as usize;
                        if final_y_pos < 0 as usize || final_y_pos >= VIDEO_HEIGHT {
                            clipped = true;
                        }

                        if current_quirks.wrap {
                            final_x_pos %= VIDEO_WIDTH;
                            final_y_pos %= VIDEO_HEIGHT;
                            clipped = false;
                        }

                        if clipped {
                            println!("display clipped");
                            continue;
                        }

                        let screen_pixel = display_data[final_y_pos][final_x_pos];
                        if is_sprite_pixel_on {
                            if screen_pixel {
                                registers[0xF] = 1;
                            }
                            display_data[final_y_pos][final_x_pos] ^= true;
                        }
                    }
                }
                display_canvas(&display_data, VIDEO_SCALE, &mut canvas);
            }
            0xE => {
                let key: usize = registers[i2] as usize;
                if i3 == 9 && i4 == 0xE {
                    if chip8_keys[key] {
                        pc += 2;
                        println!("Passed! key {} detected", key);
                        std::io::stdout().flush().unwrap();
                    } else {
                        // println!("Failed! key {} not detected", key);
                        // std::io::stdout().flush().unwrap();
                    }
                }
                if i3 == 0xA && i4 == 1 {
                    if !chip8_keys[key] {
                        pc += 2;
                        println!("Passed! key {} not detected", key);
                        std::io::stdout().flush().unwrap();
                    } else {
                        // println!("Failed! key {} detected", key);
                        // std::io::stdout().flush().unwrap();
                    }
                }
            }
            0xF => {
                if i3 == 0 && i4 == 7 {
                    registers[i2] = delay_timer.load(Ordering::Relaxed);
                }
                if i3 == 0 && i4 == 0xA {
                    let mut key_pressed = false;
                    for n in 0..chip8_keys.len() {
                        if chip8_keys[n] {
                            print!("{} detected!", n);
                            std::io::stdout().flush().unwrap();
                            registers[i2] = n as u8;
                            key_pressed = true;
                            let mut holding = key_pressed;
                            while holding {
                                for event in event_pump.poll_iter() {
                                    match event {
                                        Event::KeyUp {
                                            scancode: Some(scancode),
                                            ..
                                        } => {
                                            if let Some(chip8_key) = map_scancode_to_chip8(scancode)
                                            {
                                                chip8_keys[chip8_key as usize] = false;
                                                holding = false;
                                                println!("Key {} released", chip8_key);
                                                std::io::stdout().flush().unwrap();
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                    if !key_pressed {
                        println!("no key");
                        std::io::stdout().flush().unwrap();
                        pc -= 2;
                    }
                }
                if i3 == 1 && i4 == 5 {
                    delay_timer.store(registers[i2], Ordering::Relaxed);
                }
                if i3 == 1 && i4 == 8 {
                    sound_timer.store(registers[i2], Ordering::Relaxed);
                }
                if i3 == 1 && i4 == 0xE {
                    I += registers[i2] as u16;
                }
                if i3 == 2 && i4 == 9 {
                    let digit = registers[i2];
                    I = (FONT_SET_START_ADDRESS as u16) + ((5 * digit) as u16);
                }
                if i3 == 3 && i4 == 3 {
                    let mut value = registers[i2];
                    memory[(I + 2) as usize] = value % 10;
                    value /= 10;
                    memory[(I + 1) as usize] = value % 10;
                    value /= 10;
                    memory[(I + 0) as usize] = value % 10;
                }
                if i3 == 5 && i4 == 5 {
                    for n in 0..i2 + 1 {
                        memory[(I as usize) + n] = registers[n];
                    }
                    if current_quirks.memory_increment_by_x {
                        I += i2 as u16;
                    } else if !current_quirks.memory_leave_unchanged {
                        I += i2 as u16;
                        I += 1;
                    }
                }
                if i3 == 6 && i4 == 5 {
                    for n in 0..i2 + 1 {
                        registers[n] = memory[(I as usize) + n];
                    }

                    if current_quirks.memory_increment_by_x {
                        I += i2 as u16;
                    } else if !current_quirks.memory_leave_unchanged {
                        I += i2 as u16;
                        I += 1;
                    }
                }
            }
            _ => {
                print!("invalid instruction");
                break;
            }
        }

        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'main_loop,

                Event::KeyDown {
                    scancode: Some(scancode),
                    ..
                } => {
                    if let Some(chip8_key) = map_scancode_to_chip8(scancode) {
                        if !chip8_keys[chip8_key as usize] {
                            print!("beep {}", '\x07');
                            // device.resume();
                            chip8_keys[chip8_key as usize] = true;
                            println!("Key {} pressed", chip8_key);
                        }
                        std::io::stdout().flush().unwrap();
                    }
                }

                Event::KeyUp {
                    scancode: Some(scancode),
                    ..
                } => {
                    if let Some(chip8_key) = map_scancode_to_chip8(scancode) {
                        chip8_keys[chip8_key as usize] = false;
                        println!("Key {} released", chip8_key);
                        // device.pause();
                    }
                }

                _ => {}
            }
        }
        let time_taken = Instant::now().duration_since(start_time);
        if time_taken.as_secs_f64() < PERIOD {
            sleep(Duration::from_secs_f64(PERIOD - time_taken.as_secs_f64()));
        }
    }
    Ok(())
}
