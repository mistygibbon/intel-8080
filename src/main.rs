mod intel8080;
mod shift_register;
mod disassembler;
mod audio;

use std::{fs, thread};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::channel;
use std::time::Instant;
use sdl2::audio::{AudioCallback, AudioSpecDesired, AudioSpecWAV};
use crate::disassembler::Disassembler;
use crate::intel8080::Intel8080;
use crate::shift_register::ShiftRegister;
use sdl2::event::Event;
use sdl2::keyboard::Scancode;
use sdl2::libc::{printf, sleep, sprintf};
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{TextureQuery, WindowCanvas};
use sdl2::ttf::Sdl2TtfContext;
use sdl2::mixer::{Chunk, Channel, AUDIO_S16LSB, DEFAULT_CHANNELS, InitFlag};
use crate::audio::MySdl2Audio;

const VIDEO_WIDTH: usize = 256;
const VIDEO_HEIGHT: usize = 224;
const VIDEO_SCALE: u32 = 5;
const REFRESH_RATE: u32 = 60;

struct Playback {
    data: Arc<Mutex<Vec<u8>>>,
    pos: usize,
}

impl AudioCallback for Playback {
    type Channel = u8;

    fn callback(&mut self, out: &mut [u8]) {
        let data = self.data.lock().unwrap();
        let end = (self.pos + out.len()).min(data.len());
        let slice = &data[self.pos..end];
        out[..slice.len()].copy_from_slice(slice);
        self.pos += slice.len();
    }
}

fn sound(port:u8,data:u8,prev_data:u8,audio:&MySdl2Audio) {
    match port {
        3=>{
            if (data & 1) == 1 && prev_data & 1 == 0 {
                audio.play_ufo()
            }
            if (data & 1) == 0 && prev_data & 1 == 1 {
                audio.stop_ufo()
            }
            if (data & 2) == 2 && prev_data & 2 == 0 {
                audio.play_shoot()
            }
            if (data & 2) == 0 && prev_data & 2 == 2 {
                audio.stop_shoot()
            }
            if (data & 4) == 4 && prev_data & 4 == 0 {
                audio.play_player_die()
            }
            if (data & 8) == 8 && prev_data & 8 == 0 {
                audio.play_invader_die()
            }
        },
        5=>{
            if (data & 1) == 1 && prev_data & 1 == 0 {
                audio.play_fleet_movement_1()
            }
            if (data & 2) == 2 && prev_data & 2 == 0 {
                audio.play_fleet_movement_2()
            }
            if (data & 4) == 4 && prev_data & 4 == 0 {
                audio.play_fleet_movement_3()
            }
            if (data & 8) == 8 && prev_data & 8 == 0 {
                audio.play_fleet_movement_4()
            }
            if (data & 16) == 16 && prev_data & 16 == 0 {
                audio.play_ufo_hit()
            }
        }
        _ => {}
    }
}

fn map_scancode_to_inputs_enable(scancode: Scancode, inputs: &mut Inputs){
    match scancode {
        Scancode::A =>inputs.p1left = true,
        Scancode::D =>inputs.p1right = true,
        Scancode::F => inputs.p1shoot = true,
        Scancode::G => inputs.p1start = true,
        Scancode::C => inputs.coin = true,
        Scancode::T => inputs.p2start = true,
        Scancode::Left => inputs.p2left = true,
        Scancode::Right => inputs.p2right = true,
        Scancode::Slash => inputs.p2shoot = true,
        _ => {}
    }
}

fn map_scancode_to_inputs_disable(scancode: Scancode, inputs: &mut Inputs){
    match scancode {
        Scancode::A =>inputs.p1left = false,
        Scancode::D =>inputs.p1right = false,
        Scancode::F => inputs.p1shoot = false,
        Scancode::G => inputs.p1start = false,
        Scancode::C => inputs.coin = false,
        Scancode::T => inputs.p2start = false,
        Scancode::Left => inputs.p2left = false,
        Scancode::Right => inputs.p2right = false,
        Scancode::Slash => inputs.p2shoot = false,
        _ => {}
    }
}

struct Inputs {
    p2start: bool,
    p1start: bool,
    p1shoot: bool,
    p1left: bool,
    p1right: bool,
    p2shoot: bool,
    p2left: bool,
    p2right: bool,
    coin: bool,
}

impl Inputs {
    fn new() -> Inputs {
        Inputs{
            p2start:false,
            p1start: false,
            p1shoot: false,
            p1left: false,
            p1right: false,
            p2shoot: false,
            p2left: false,
            p2right: false,
            coin: false,
        }
    }
}

fn input0(inputs: &Inputs) ->u8{
    let mut result = 0b00001110;
    if inputs.p1shoot {result |= 1<<4} else {result &= !(1<<4)};
    if inputs.p1left {result |= 1<<5} else {result &= !(1<<5)};
    if inputs.p1right {result |= 1<<6} else {result &= !(1<<6)};
    result
}


fn input1(inputs: &Inputs)->u8{
    let mut result = 0b00001000;
    if inputs.coin{result|=1} else  {result &= !(1)};
    if inputs.p2start {result |= 1<<1} else {result &= !(1<<1)};
    if inputs.p1start {result |= 1<<2} else {result &= !(1<<2)};
    if inputs.p1shoot {result |= 1<<4} else {result &= !(1<<4)};
    if inputs.p1left {result |= 1<<5} else {result &= !(1<<5)};
    if inputs.p1right {result |= 1<<6} else {result &= !(1<<6)};
    result
}
fn input2(inputs: &Inputs)->u8 {
    let mut result = 0b00000011;
    if inputs.p2shoot {result |= 1<<4} else {result &= !(1<<4)};
    if inputs.p2left {result |= 1<<5} else {result &= !(1<<5)};
    if inputs.p2right {result |= 1<<6} else {result &= !(1<<6)};
    result
}

pub fn display_canvas(display_data: &[u8; VIDEO_WIDTH * VIDEO_HEIGHT / 8], scale: u32, canvas: &mut WindowCanvas) {
    canvas.set_draw_color(Color::RGB(0, 0, 0));
    canvas.clear();
    canvas.set_draw_color(Color::RGB(255, 255, 255));
    for (index, byte) in display_data.iter().enumerate() {
        let mut byte_arr:[u8;8] = [0;8];
        for n in 0..8 {
            let bit = (byte & (0b01 << n))>>(n) ;

            let total_index = 8 * index + n;
            let x = total_index % 256;
            let y = total_index / 256;
            let rotated_x = y;
            let rotated_y = VIDEO_WIDTH - x;

            if bit == 1 {
                let rect = Rect::new(
                    (rotated_x as u32 * scale) as i32,
                    ((rotated_y) as u32 * scale) as i32,
                    scale,
                    scale,
                );
                canvas.fill_rect(rect).unwrap();
            }
        }

    }
    canvas.present();
}

fn main() -> Result<(), String> {

    let sdl_context = sdl2::init()?;
    let video = sdl_context.video()?;
    let audio_subsystem = sdl_context.audio()?;
    let window = video
        .window(
            "Intel 8080 Emulator",
            (VIDEO_HEIGHT as u32) * VIDEO_SCALE,
            (VIDEO_WIDTH as u32) * VIDEO_SCALE,
        )
        .position_centered()
        .build()
        .map_err(|e| e.to_string())?;
    let mut canvas = window.into_canvas().build().map_err(|e| e.to_string())?;
    let mut event_pump = sdl_context.event_pump()?;

    // let wav_spec = AudioSpecWAV::load_wav("samples/0.wav").expect("Failed to load sample file");
    // let data = Arc::new(Mutex::new(wav_spec.buffer().to_vec()));
    // let desired_spec = AudioSpecDesired {
    //     freq: Some(wav_spec.freq),
    //     channels: Some(wav_spec.channels),
    //     samples: None,
    // };
    // let device = audio_subsystem.open_playback(None, &desired_spec, |_| Playback {
    //     data: data.clone(),
    //     pos: 0,
    // })?;

    // let frequency = 44_100;
    // let format = AUDIO_S16LSB; // signed 16 bit samples, in little-endian byte order
    // let channels = 8;
    // let chunk_size = 1_024;
    // sdl2::mixer::open_audio(frequency, format, channels, chunk_size)?;
    // sdl2::mixer::init(InitFlag::MID);
    // sdl2::mixer::allocate_channels(channels);
    
    // Start playback
    // let chunk = Chunk::from_file("samples/1.wav")?;
    // Channel(7).play(&chunk, 0).unwrap();

    let audio = MySdl2Audio::new(
        &fs::read("samples/0.wav").unwrap() ,
        &fs::read("samples/1.wav").unwrap(),
        &fs::read("samples/2.wav").unwrap(),
        &fs::read("samples/3.wav").unwrap(),
        &fs::read("samples/4.wav").unwrap(),
        &fs::read("samples/5.wav").unwrap(),
        &fs::read("samples/6.wav").unwrap(),
        &fs::read("samples/7.wav").unwrap(),
        &fs::read("samples/8.wav").unwrap(),
        &fs::read("samples/9.wav").unwrap(),
    );

    // let ttf_context = sdl2::ttf::init().map_err(|e| e.to_string())?;
    // let font_path = Path::new("/usr/share/fonts/open-sans/OpenSans-Regular.ttf");
    // let font = ttf_context.load_font(font_path, 24)?;
    // let texture_creator = canvas.texture_creator();

    let mut shift_register = ShiftRegister::new();


    let mut intel8080 = Intel8080::new();
    let prog = fs::read("cpu_tests/invaders.concatenated").expect("Unable to read file");
    // let mut zeroes = vec![0u8; 0x100];
    // zeroes.extend(prog);
    // zeroes[0] = 0b11000011;
    // zeroes[1] = 0x00;
    // zeroes[2] = 0x01;
    intel8080.load_program(prog);

    let begin = Instant::now();
    let mut last_interrupt = Instant::now();
    let mut count = 0;
    let mut count_interrupt = 1;
    let mut countp = 0;
    struct Avg{avg:f64,n:usize};
    let mut avg_time = Avg{avg: 0.0,n:0};
    let mut inputs = Inputs::new();
    let mut prev_port3:u8=0;
    let mut prev_port5:u8=0;

    // Render text to a surface, then to a texture
    // let surface = font
    //     .render(&format!("Cycles: {} Program counter: {} Clock speed: {}",intel8080.total_cycle, intel8080.PC,intel8080.total_cycle as f64 / begin.elapsed().as_secs_f64()))
    //     .blended(Color::WHITE)
    //     .map_err(|e| e.to_string())?;
    // let texture = texture_creator
    //     .create_texture_from_surface(&surface)
    //     .map_err(|e| e.to_string())?;
    //
    // let TextureQuery { width, height, .. } = texture.query();
    //
    // // Position for text
    // let target = Rect::new(100, 100, width, height);
    //
    // // canvas.set_draw_color(Color::BLACK);
    // // canvas.clear();
    // // canvas.copy(&texture, None, Some(target))?;
    // // canvas.present();
    'main_loop: loop {
        intel8080.cycle();
        if intel8080.oport.len() > 0 {
            let (port, data) = intel8080.oport.pop().unwrap();
            match port {
                2 => shift_register.write_offset(data),
                3 => {sound(port,data,prev_port3,&audio);prev_port3 = data;},
                4 => shift_register.insert(data),
                5 => {sound(port,data,prev_port5,&audio);prev_port5 = data;},

                _ => {}
            }
        }

        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'main_loop,

                Event::KeyDown {
                    scancode: Some(scancode),
                    ..
                } => {
                    map_scancode_to_inputs_enable(scancode, &mut inputs);
                }

                Event::KeyUp {
                    scancode: Some(scancode),
                    ..
                } => {
                    map_scancode_to_inputs_disable(scancode, &mut inputs);
                }

                _ => {}
            }
        }
        intel8080.iport[0] = input0(&inputs);
        intel8080.iport[1] = input1(&inputs);
        intel8080.iport[2] = input2(&inputs);
        intel8080.iport[3] = shift_register.result();

        if (intel8080.total_ticks >33333*count as usize) {
            count += 1;
            // println!("display");
            let display_data = &intel8080.memory[0x2400..=0x3FFF];
            // for data in display_data {
            //     if *data != 0 {
            //         println!("Display data exist!!! {}",*data);
            //     }
            // }
            display_canvas(display_data.try_into().expect(""),VIDEO_SCALE,&mut canvas);
            intel8080.interrupt_data.push(0b11010111);
        }

        if (intel8080.total_ticks > 1000000*countp as usize){
            countp+=1;
            // println!("{}", intel8080.total_ticks as f32 / begin.elapsed().as_secs_f32());
        }

        if intel8080.total_ticks >16666*count_interrupt && intel8080.interrupt_enabled {
            count_interrupt += 2;
            avg_time.avg = (avg_time.avg * avg_time.n as f64 + last_interrupt.elapsed().as_secs_f64()) / (avg_time.n+1) as f64;
            avg_time.n += 1;
            // println!("Avg: {}", avg_time.avg);
            intel8080.interrupt_data.push(0b11001111);
            last_interrupt = Instant::now();
        }

        intel8080.last_cycle_time = Instant::now();
        //
        // if (intel8080.total_ticks>100000){
        //     break;
        // }

        // port_operations(&mut intel8080);
    }

    Ok(())
}