use std::io::{stdout, Write};
use std::thread::sleep;
use std::time::{Duration, Instant};
use log::warn;
use spin_sleep::SpinSleeper;

const PROGRAM_START_ADDRESS: usize = 0x0;

// const CLOCK_SPEED:usize = 2000000;
// const PERIOD:f64 = 1.0 / (CLOCK_SPEED as f64);
const PERIOD_NS:usize = 350; // should be 500, but 350 works to get 2MHz

pub trait IOHandler {
    fn input(&mut self, port: u8) -> u8;
    fn output(&mut self, port: u8, value: u8);
}

pub struct Registers {
    A:u8,
    Flags: u8,
    B:u8,
    C:u8,
    D:u8,
    E:u8,
    H:u8,
    L:u8,
}

impl Registers {
    pub fn new() -> Registers {
        Self {
            A:0,
            Flags:0b00000010,
            B:0,
            C:0,
            D:0,
            E:0,
            H:0,
            L:0,
        }
    }
}

pub struct Intel8080 {
    pub memory:[u8;65536],
    pub PC:u16,
    registers: Registers,
    ticks: usize,
    pub total_ticks: usize,
    pub last_cycle_time: Instant,
    SP:u16,
    rp:u8,
    ddd:u8,
    sss:u8,
    cc:u8,
    alu:u8,
    n:u8,
    pub oport: Vec<(u8,u8)>,
    pub iport:[u8;256],
    pub interrupt_enabled: bool,
    // interrupt_requested: bool,
    // interrupt_acknowledge: bool,
    pub interrupt_data: Vec<u8>,
    sleeper: SpinSleeper,
}

impl Intel8080 {
    pub fn new() -> Intel8080 {
        Self {
            memory: [0;65536],
            PC: PROGRAM_START_ADDRESS as u16,
            registers: Registers::new(),
            ticks: 0,
            total_ticks: 0,
            last_cycle_time: Instant::now(),
            SP:0,
            rp:0,
            ddd:0,
            sss:0,
            cc:0,
            alu:0,
            n:0,
            oport:Vec::new(),
            iport:[0;256],
            interrupt_enabled: true,
            // interrupt_requested: false,
            // interrupt_acknowledge: false,
            interrupt_data: Vec::new(),
            sleeper: SpinSleeper::new(50000000).with_spin_strategy(spin_sleep::SpinStrategy::SpinLoopHint),
        }
    }

    pub fn load_program(&mut self, data: Vec<u8>) {
        // let mut data = Vec::new();
        // data.push(0x01);
        // data.push(0x02);
        // data.push(0x03);

        for n in 0..data.len(){
            self.memory[n] = data[n];
        }

        // for n in 0..256 {
        //     self.memory[n] = n as u8;
        // }
    }

    fn get_m(&self) -> u8{
        let address:usize = ((self.registers.H as usize) << 8) + (self.registers.L as usize);
        return self.memory[address];
    }

    fn write_m(&mut self, value: u8){
        let address:usize = ((self.registers.H as usize) << 8) + (self.registers.L as usize);
        self.memory[address] = value;
    }
    
    fn add_3_szapc(&mut self, i1: u8, i2:u8, i3:u8) -> u8 {
        let result = i1.wrapping_add(i2).wrapping_add(i3);
        self.set_szp(result);

        // a
        let ac_result = (i1 & 0x0F) + (i2 & 0x0F) + (i3 & 0x0F);
        if (ac_result & 0x10) >> 4 > 0 {
            self.registers.Flags |= 0b00010000;
        } else {
            self.registers.Flags &= !(0b00010000);
        }
        
        // c
        if (i1 as u16) + (i2 as u16) + (i3 as u16) > 0xFF {
            self.registers.Flags |= 0b00000001;
        } else {
            self.registers.Flags &= !(0b00000001);
        }
        result
    }

    fn add_szap(&mut self, i1:u8,i2:u8) -> u8{
        let result = i1.wrapping_add(i2);
        self.set_szp(result);

        // a
        let ac_result = (i1 & 0x0F) + (i2 & 0x0F);
        if (ac_result & 0x10) >> 4 == 1 {
            self.registers.Flags |= 0b00010000;
        } else {
            self.registers.Flags &= !(0b00010000);
        }

        return result;
    }

    fn add_szapc(&mut self, i1:u8,i2:u8) -> u8{
        let result = self.add_szap(i1, i2);
        self.set_c_add(i1,i2);
        return result;
    }

    fn sub_szap(&mut self, i1:u8,i2:u8) -> u8{
        let complement = (!i2).wrapping_add(1);
        let result = i1.wrapping_sub(i2);
        self.set_szp(result);

        // a
        if ((complement & 0x0F) + (i1 & 0x0F) > 0x0F ) {
            self.registers.Flags |= 0b00010000;
        } else {
            self.registers.Flags &= !(0b00010000);
        }

        return result;
    }

    fn sub_szapc(&mut self, i1:u8,i2:u8) -> u8{
        let result = self.sub_szap(i1, i2);
        self.set_c_sub(i1,i2);
        return result;
    }

    // fn set_szapc(&mut self, i1:u8, i2:u8, result:u8){
    //     log::warn!("Wrong method");
    //     self.set_szp(i1, i2, result);
    //     self.set_c_add(i1,i2);
    // }

    fn get_s(&self)->bool{
        if (self.registers.Flags & 0x80)>>7==1 {
            return true;
        } else {
            return false;
        }
    }

    fn get_z(&self)->bool{
        if (self.registers.Flags & 0x40)>>6==1 {
            return true;
        } else {
            return false;
        }
    }

    fn get_a(&self)->bool{
        if (self.registers.Flags & 0x10)>>4==1 {
            return true;
        } else {
            return false;
        }
    }

    fn get_p(&self)->bool{
        if (self.registers.Flags & 0x04)>>2==1 {
            return true;
        } else {
            return false;
        }
    }

    fn get_c(&self)->bool{
        if (self.registers.Flags & 0x01)==1 {
            return true;
        } else {
            return false;
        }
    }
    
    fn write_a(&mut self, val:bool){
        if val {
            self.registers.Flags |= 0b00010000;
        } else {
            self.registers.Flags &= !(0b00010000);
        }
    }
    
    fn write_c(&mut self, val:bool){
        if val {
            self.registers.Flags |= 0x01;
        } else {
            self.registers.Flags &= !(0x01);
        }
    }

    fn set_szp(&mut self, result: u8) {
        // s
        if (result & 0x80) >> 7 == 1 {
            self.registers.Flags |= 0b10000000;
        } else {
            self.registers.Flags &= !(0b10000000);
        }

        // z
        if result == 0 {
            self.registers.Flags |= 0b01000000;
        } else {
            self.registers.Flags &= !(0b01000000);
        }

        // a
        // let ac_result = (i1 & 0x0F) + (i2 & 0x0F);
        // if (ac_result & 0x10) >> 4 == 1 {
        //     self.registers.Flags |= 0b00010000;
        // } else {
        //     self.registers.Flags &= !(0b00010000);
        // }

        // p
        let mut parity: bool = true;
        for n in 0..8 {
            if ((result >> n) & 0x01) == 1 {
                parity = !parity;
            }
        }
        if parity {
            self.registers.Flags |= 0b00000100;
        } else {
            self.registers.Flags &= !(0b00000100);
        }
    }

    fn set_c_add(&mut self, i1: u8, i2: u8){
        // c
        if (i1 as u16) + (i2 as u16) > 0xFF {
            self.registers.Flags |= 0b00000001;
        } else {
            self.registers.Flags &= !(0b00000001);
        }
    }
    fn set_c_sub(&mut self, i1: u8, i2: u8){
        // c
        let complement = (!i2).wrapping_add(1);
        if (i1 as u16) + complement as u16 > 0xFF {
            self.registers.Flags |= 0b00000001;
        } else {
            self.registers.Flags &= !(0b00000001);
        }
    }


    fn get_register(&self, num:u8)->u8{
        match num {
            0=>self.registers.B,
            1=>self.registers.C,
            2=>self.registers.D,
            3=>self.registers.E,
            4=>self.registers.H,
            5=>self.registers.L,
            6=>self.get_m(),
            7=>self.registers.A,
            _ => {return 0}
        }
    }

    fn write_register(&mut self, num:u8, val:u8){
        match num {
            0=>self.registers.B=val,
            1=>self.registers.C=val,
            2=>self.registers.D=val,
            3=>self.registers.E=val,
            4=>self.registers.H=val,
            5=>self.registers.L=val,
            6=>self.write_m(val),
            7=>self.registers.A=val,
            _ => {}
        }
    }

    fn get_bc(&self)->u16{
        ((self.get_register(0) as u16) << 8) + (self.get_register(1) as u16)
    }
    fn get_de(&self)->u16{ ((self.get_register(2) as u16) << 8) + (self.get_register(3) as u16) }
    fn get_hl(&self)->u16{
        ((self.get_register(4) as u16) << 8) + (self.get_register(5) as u16)
    }
    fn get_psw(&self)->u16{
        ((self.get_register(7) as u16) << 8) + (self.registers.Flags as u16)
    }
    fn get_sp(&self)->u16{
        return self.SP;
    }

    fn write_bc(&mut self, val:u16){
        self.write_register(0, ((val & 0xFF00)>>8) as u8);
        self.write_register(1, (val & 0x00FF) as u8);
    }
    fn write_de(&mut self, val:u16){
        self.write_register(2, ((val & 0xFF00)>>8) as u8);
        self.write_register(3, (val & 0x00FF) as u8);
    }
    fn write_hl(&mut self, val:u16){
        self.write_register(4, ((val & 0xFF00)>>8) as u8);
        self.write_register(5, (val & 0x00FF) as u8);
    }
    fn write_sp(&mut self, val:u16){
        self.SP=val;
    }

    fn read_next_byte(&mut self) -> u8 {
        let val =  self.memory[self.PC as usize];
        self.PC += 1;
        val
    }

    pub fn cycle(&mut self) {
        // print!("{:x}\n",self.PC);
        if (self.PC==5 && !cfg!(test)){
            if self.registers.C == 9 {
                let mut addr = self.get_de();
                while (self.memory[addr as usize]!=0b00100100){
                    print!("{}", self.memory[addr as usize] as char);
                    // stdout().flush();
                    addr += 1;
                }
            } else if self.registers.C==2 {
                print!("{}", self.registers.E as char);
                // stdout().flush();
            }
            self.decode_execute(0b11001001); //return
        }

        let opcode = self.fetch();
        self.decode_execute(opcode);
        
        self.total_ticks += self.ticks;
        let cycle_time:u128 = ((self.ticks) * PERIOD_NS) as u128;
        let duration = self.last_cycle_time.elapsed().as_nanos();
        if cycle_time> duration {
            self.sleeper.sleep(Duration::from_nanos((cycle_time - duration) as u64));
        }
        self.ticks = 0;
        
        if (self.interrupt_enabled && self.interrupt_data.len() > 0){
            let interrupt_begin_time = Instant::now();
        
            // self.interrupt_acknowledge = true;
            // sleep(Duration::from_secs_f64(2.0*PERIOD));
        
            let opcode = self.interrupt_data.pop().unwrap();
            self.interrupt_enabled = false;
            // self.interrupt_acknowledge = false;
        
            self.decode_execute(opcode);
            // self.interrupt_enabled = true;
        
            let duration = interrupt_begin_time.elapsed().as_nanos();
            self.total_ticks += self.ticks;
            let cycle_time:u128 = ((self.ticks) * PERIOD_NS) as u128;
            if cycle_time> duration as u128 {
                self.sleeper.sleep(Duration::from_nanos((cycle_time - duration) as u64))
            }
            self.ticks = 0;
        }
        
        // self.last_cycle_time = Instant::now();

        // self.check_flags();
    }

    fn check_flags(&mut self){
        let flags = self.registers.Flags;
        let mut bit_arr:[u8;8] = [0;8];
        for n in 0..8 {
            bit_arr[n] = (flags & (0b01 << (7-n)))>>(7-n) ;
        }
        if bit_arr[2] != 0 {
            println!("flag bit 2 mismatch")
        }
        if bit_arr[4] != 0 {
            println!("flag bit 4 mismatch")
        }
        if bit_arr[6] != 1 {
            println!("flag bit 6 mismatch")
        }
    }

    fn fetch(&mut self)->u8{
        let opcode = self.read_next_byte();
        opcode
    }

    fn decode_execute(&mut self, opcode:u8) {
        // let opcode = self.memory[self.PC as usize];
        // self.PC = self.PC.wrapping_add(1);
        let mut opcode_arr:[u8;8] = [0;8];
        for n in 0..8 {
            opcode_arr[n] = (opcode & (0b01 << (7-n)))>>(7-n) ;
        }
        self.rp = (opcode & 0x30) >> 4;
        self.ddd = (opcode & 0x38)>> 3;
        self.sss = opcode & 0x07;
        self.cc = self.ddd;
        self.alu = self.ddd;
        self.n = self.ddd;

        // println!("{:?}", opcode_arr);
        // std::io::stdout().flush().unwrap();

        match opcode_arr {
            [0,0,0,0,0,1,1,1]=>self.rlc(),
            [0,0,0,0,1,1,1,1]=>self.rrc(),
            [0,0,0,1,0,1,1,1]=>self.ral(),
            [0,0,0,1,1,1,1,1]=>self.rar(),
            [0,0,1,0,0,0,1,0]=>self.shld(),
            [0,0,1,0,0,1,1,1]=>self.daa(),
            [0,0,1,0,1,0,1,0]=>self.lhld(),
            [0,0,1,0,1,1,1,1]=>self.cma(),
            [0,0,1,1,0,0,1,0]=>self.sta(),
            [0,0,1,1,0,1,1,1]=>self.stc(),
            [0,0,1,1,1,0,1,0]=>self.lda(),
            [0,0,1,1,1,1,1,1]=>self.cmc(),
            [0,1,1,1,0,1,1,0]=>self.hlt(),
            [1,1,0,0,0,0,1,1]=>self.jmp(),
            [1,1,0,0,1,0,1,1]=>self.jmp(), // alternative
            [1,1,0,0,1,0,0,1]=>self.ret(),
            [1,1,0,1,1,0,0,1]=>self.ret(), // alternative
            [1,1,0,0,1,1,0,1]=>self.call(),
            [1,1,0,1,1,1,0,1]=>self.call(), // alternative
            [1,1,1,0,1,1,0,1]=>self.call(), // alternative
            [1,1,1,1,1,1,0,1]=>self.call(), // alternative
            [1,1,0,1,0,0,1,1]=>self.out_port(),
            [1,1,0,1,1,0,1,1]=>self.in_port(),
            [1,1,1,0,0,0,1,1]=>self.xthl(),
            [1,1,1,0,1,0,0,1]=>self.pchl(),
            [1,1,1,0,1,0,1,1]=>self.xchg(),
            [1,1,1,1,0,0,1,1]=>self.di(),
            [1,1,1,1,1,0,0,1]=>self.sphl(),
            [1,1,1,1,1,0,1,1]=>self.ei(),

            [0,0,_,_,0,0,0,0] => self.nop(),
            [0,0,_,_,1,0,0,0] => self.nop(), // alternative
            [0,0,r1,r0,0,0,0,1] => self.lxi(r1,r0),
            [0,0,r1,r0,0,0,1,0]=>self.stax(r1,r0),
            [0,0,r1,r0,0,0,1,1]=>self.inx(r1,r0),
            [0,0,d2,d1,d0,1,0,0]=>self.inr(d2,d1,d0),
            [0,0,d2,d1,d0,1,0,1]=>self.dcr(d2,d1,d0),
            [0,0,d2,d1,d0,1,1,0]=>self.mvi(d2,d1,d0),
            [0,0,r1,r0,1,0,0,1]=>self.dad(r1,r0),
            [0,0,r1,r0,1,0,1,0]=>self.ldax(r1,r0),
            [0,0,r1,r0,1,0,1,1]=>self.dcx(r1,r0),
            [0,1,d2,d1,d0,s2,s1,s0]=>self.mov(d2,d1,d0,s2,s1,s0),
            [1,0,alu2,alu1,alu0,s2,s1,s0]=>self.aluop1(alu2,alu1,alu0,s2,s1,s0),
            [1,1,c2,c1,c0,0,0,0]=>self.rcc(c2,c1,c0),
            [1,1,r1,r0,0,0,0,1]=>self.pop(r1,r0),
            [1,1,c2,c1,c0,0,1,0]=>self.jcc(c2,c1,c0),
            [1,1,c2,c1,c0,1,0,0]=>self.ccc(c2,c1,c0),
            [1,1,r1,r0,0,1,0,1]=>self.push(r1,r0),
            [1,1,alu2,alu1,alu0,1,1,0]=>self.aluop2(alu2,alu1,alu0),
            [1,1,n2,n1,n0,1,1,1]=>self.rst(n2,n1,n0),
            _ => {
                println!("invalid opcode: {:#b} {:#x} ", opcode, opcode);
            }
        }
    }

    fn nop(&mut self) {
        self.ticks += 4;
    }

    fn lxi(&mut self, r1:u8, r0:u8) {
        self.ticks +=10;
        let datalo = self.read_next_byte();
        let datahi = self.read_next_byte();
        let data = u16::from_le_bytes([datalo,datahi]);
        match self.rp {
            0=>{
                self.write_bc(data);
            }
            1=>{
                self.write_de(data)
            }
            2=>{
                self.write_hl(data)
            }
            3=>{
                self.write_sp(data)
            }
            _ => {

            }
        }
    }

    fn stax(&mut self,r1:u8, r0:u8) {
        self.ticks += 7;
        match self.rp {
            0=>self.memory[self.get_bc() as usize] = self.registers.A,
            1=>self.memory[self.get_de() as usize] = self.registers.A,
            _=>{}
        }
    }

    fn inx(&mut self, r1:u8, r0:u8) {
        self.ticks += 5;
        match self.rp {
            0=>{
                let result = self.get_bc().wrapping_add(1);
                self.write_bc(result);
            }
            1=>{
                let result = self.get_de().wrapping_add(1);
                self.write_de(result);
            }
            2=>{
                let result = self.get_hl().wrapping_add(1);
                self.write_hl(result);
            }
            3=>{
                self.SP = self.SP.wrapping_add(1);
            }
            _ => {
                return;
            }
        }
    }

    fn inr(&mut self,d2:u8, d1:u8,d0:u8) {
        self.ticks += 5;
        match self.ddd {
            0=>self.registers.B = self.add_szap(self.registers.B,1),
            1=>self.registers.C = self.add_szap(self.registers.C,1),
            2=>self.registers.D = self.add_szap(self.registers.D,1),
            3=>self.registers.E = self.add_szap(self.registers.E,1),
            4=>self.registers.H = self.add_szap(self.registers.H,1),
            5=>self.registers.L = self.add_szap(self.registers.L,1),
            6=>{
                self.ticks += 5;
                let sum:u8 = self.add_szap(self.get_m(),1);
                self.write_m(sum);
            },
            7=>self.registers.A = self.add_szap(self.registers.A,1),
            _ => {println!("invalid inr operation:"); }
        }
    }
    fn dcr(&mut self,d2:u8, d1:u8,d0:u8) {
        self.ticks += 5;
        match self.ddd {
            0=>self.registers.B = self.sub_szap(self.registers.B,1),
            1=>self.registers.C = self.sub_szap(self.registers.C,1),
            2=>self.registers.D = self.sub_szap(self.registers.D,1),
            3=>self.registers.E = self.sub_szap(self.registers.E,1),
            4=>self.registers.H = self.sub_szap(self.registers.H,1),
            5=>self.registers.L = self.sub_szap(self.registers.L,1),
            6=>{
                self.ticks += 5;
                let diff:u8 = self.sub_szap(self.get_m(),1);
                self.write_m(diff);
            },
            7=>self.registers.A = self.sub_szap(self.registers.A,1),
            _ => {}
        }
    }
    fn mvi(&mut self,d2:u8,d1:u8,d0:u8) {
        self.ticks += 7;
        let data:u8 = self.read_next_byte();

        match self.ddd {
            0=>self.registers.B = data,
            1=>self.registers.C = data,
            2=>self.registers.D = data,
            3=>self.registers.E = data,
            4=>self.registers.H = data,
            5=>self.registers.L = data,
            6=>{
                self.ticks += 3;
                self.write_m(data);
            },
            7=>self.registers.A = data,
            _ => {}
        }
    }
    fn dad(&mut self,r1:u8,r0:u8) {
        self.ticks += 10;

        let hl:u16 = self.get_hl();
        let result: u16;
        match self.rp {
            0=>result = hl.wrapping_add(self.get_bc()),
            1=>result = hl.wrapping_add(self.get_de()),
            2=>result = hl.wrapping_add(self.get_hl()),
            3=>result = hl.wrapping_add(self.SP),
            _ => {result=0}
        }
        if (result < hl) {
            self.registers.Flags |= 1;
        } else {
            self.registers.Flags &= !1;
        }
        self.write_hl(result);
    }
    fn ldax(&mut self,r1:u8,r0:u8) {
        self.ticks += 7;
        match self.rp {
            0=>{
                let bc = self.get_bc();
                self.registers.A = self.memory[bc as usize];
            }
            1=>{
                let de = self.get_de();
                self.registers.A = self.memory[de as usize];
            }
            _ => {}
        }
    }
    fn dcx(&mut self,r1:u8,r0:u8) {
        self.ticks += 5;
        match self.rp {
            0=>{
                let result = self.get_bc().wrapping_sub(1);
                self.write_bc(result);
            }
            1=>{
                let result = self.get_de().wrapping_sub(1);
                self.write_de(result);
            }
            2=>{
                let result = self.get_hl().wrapping_sub(1);
                self.write_hl(result);
            }
            3=>{
                self.SP = self.SP.wrapping_sub(1);
            }
            _ => {
                return;
            }
        }
    }
    
    fn rlc(&mut self) {
        self.ticks += 4;
        let first_bit:u8 = (self.registers.A & 0x80) >> 7;
        self.registers.A = self.registers.A << 1;
        self.registers.A |= first_bit;
        if first_bit == 0x01 {
            self.registers.Flags |= 0x01;
        }else {
            self.registers.Flags &= !0x01;
        }
    }
    
    fn rrc(&mut self) {
        self.ticks += 4;
        let first_bit:u8 = self.registers.A & 0x01;
        self.registers.A = self.registers.A >> 1;
        self.registers.A |= first_bit<<7;
        if first_bit == 0x01 {
            self.registers.Flags |= 0x01;
        }else {
            self.registers.Flags &= !0x01;
        }
    }
    
    fn ral(&mut self) {
        self.ticks += 4;
        let first_bit:u8 = (self.registers.A & 0x80) >> 7;
        self.registers.A = self.registers.A << 1;

        let carry:u8 = self.registers.Flags & 0x01;
        self.registers.A |= carry;

        if first_bit == 0x01 {
            self.registers.Flags |= 0x01;
        }else {
            self.registers.Flags &= !0x01;
        }
    }
    
    fn rar(&mut self) {
        self.ticks += 4;
        let first_bit:u8 = self.registers.A & 0x01;
        self.registers.A = self.registers.A >> 1;

        let carry:u8 = self.registers.Flags & 0x01;
        self.registers.A |= carry<<7;

        if first_bit == 0x01 {
            self.registers.Flags |= 0x01;
        }else {
            self.registers.Flags &= !0x01;
        }
    }
    fn shld(&mut self) {
        self.ticks += 16;
        let addlo = self.read_next_byte();
        let addhi = self.read_next_byte();
        self.memory[((addhi as usize) << 8) + (addlo as usize)] = self.registers.L;
        self.memory[((addhi as usize) << 8) + (addlo as usize) + 1] = self.registers.H;
    }

    // TODO need fix
    fn daa(&mut self) {
        self.ticks += 4;
        let A = self.registers.A;
        let AC = (self.registers.Flags & 0b00010000)>>4;
        let mut add = 0;
        let msb = (self.registers.A)>>4;
        let lsb = A & 0b00001111;
        if (A & 0b00001111)>9 || AC == 1 {
            add = 6;
        }
        
        let C = self.registers.Flags & 0x01;
        if msb > 9 || C == 1 || (msb >= 9 && lsb>9) {
            add += 0x60;
            self.registers.Flags |= 0x01;
        }
        self.registers.A = self.add_szap(self.registers.A, add);
    }
    fn lhld(&mut self) {
        self.ticks += 16;
        let addlo = self.read_next_byte();
        let addhi = self.read_next_byte();
        self.registers.L = self.memory[((addhi as usize) << 8) + (addlo as usize)] ;
        self.registers.H = self.memory[((addhi as usize) << 8) + (addlo as usize) + 1];
    }

    // TODO need fix
    fn cma(&mut self) {
        self.ticks += 4;
        self.registers.A = !self.registers.A;
    }
    fn sta(&mut self) {
        self.ticks += 13;
        let addlo = self.read_next_byte();
        let addhi = self.read_next_byte();
        self.memory[((addhi as usize) << 8) + (addlo as usize)] = self.registers.A;
    }

    // TODO need fix
    fn stc(&mut self) {
        self.ticks += 4;
        self.registers.Flags |= 0x01;
    }
    fn lda(&mut self) {
        self.ticks += 13;
        let addlo = self.read_next_byte();
        let addhi = self.read_next_byte();
        self.registers.A = self.memory[((addhi as usize) << 8) + (addlo as usize)] ;
    }

    fn cmc(&mut self) {
        self.ticks += 4;
        if self.registers.Flags & 0x01 == 1 {
            self.registers.Flags &= !0x01;
        } else {
            self.registers.Flags |= 0x01;
        }
    }
    fn mov(&mut self,d2:u8,d1:u8,d0:u8,s2:u8,s1:u8,s0:u8) {
        self.ticks += 5;
        let reg_val:u8;
        match (s2,s1,s0) {
            (0,0,0)=>reg_val=self.registers.B,
            (0,0,1)=>reg_val=self.registers.C,
            (0,1,0)=>reg_val=self.registers.D,
            (0,1,1)=>reg_val=self.registers.E,
            (1,0,0)=>reg_val=self.registers.H,
            (1,0,1)=>reg_val=self.registers.L,
            (1,1,0)=>{reg_val=self.get_m();self.ticks += 2;},
            (1,1,1)=>reg_val=self.registers.A,
            _ => {reg_val=0}
        }
        match (d2,d1,d0) {
            (0,0,0)=>self.registers.B=reg_val,
            (0,0,1)=>self.registers.C=reg_val,
            (0,1,0)=>self.registers.D=reg_val,
            (0,1,1)=>self.registers.E=reg_val,
            (1,0,0)=>self.registers.H=reg_val,
            (1,0,1)=>self.registers.L=reg_val,
            (1,1,0)=>{self.write_m(reg_val);self.ticks +=2;},
            (1,1,1)=>self.registers.A=reg_val,
            _ => {}
        }
    }
    fn hlt(&mut self) {
        print!("Halt");
        self.ticks += 7;
        self.PC = self.PC.wrapping_sub(1);
    }

    // TODO need fix
    fn aluop1(&mut self,alu2:u8,alu1:u8,alu0:u8,s2:u8,s1:u8,s0:u8) {
        self.ticks += 4;
        let a_val:u8 = self.registers.A;
        let reg_val:u8;
        match (s2,s1,s0) {
            (0,0,0)=>reg_val=self.registers.B,
            (0,0,1)=>reg_val=self.registers.C,
            (0,1,0)=>reg_val=self.registers.D,
            (0,1,1)=>reg_val=self.registers.E,
            (1,0,0)=>reg_val=self.registers.H,
            (1,0,1)=>reg_val=self.registers.L,
            (1,1,0)=>{reg_val=self.get_m();self.ticks += 3;},
            (1,1,1)=>reg_val=self.registers.A,
            _ => {reg_val=0}
        }
        match (alu2,alu1,alu0) {
            (0,0,0)=>self.registers.A=self.add_szapc(a_val,reg_val),
            (0,0,1)=>{
                self.registers.A=self.add_3_szapc(a_val,reg_val,self.registers.Flags & 0x01);
            },
            (0,1,0)=>{
                self.registers.A=self.add_3_szapc(a_val,!reg_val,1);
                self.write_c(reg_val as u16 > a_val as u16) // overwrite c,
            },
            (0,1,1)=>{
                let complement = (!reg_val); // 2s complement
                let carry = self.registers.Flags & 0x01;
                self.registers.A=self.add_3_szapc(a_val,complement,if carry == 1 { 0 } else { 1 });
                self.write_c(reg_val as u16 + (carry as u16) > a_val as u16);
            },
            (1,0,0)=>{
                self.set_szp(a_val&reg_val);
                self.registers.Flags&=0b11101110;
                if (((self.registers.A | reg_val) & 0x08) != 0){
                    self.registers.Flags |= 0b00010000;
                } else {
                    self.registers.Flags &= !0b00010000;
                }
                self.registers.A=a_val&reg_val;
            },
            (1,0,1)=>{
                self.registers.A=a_val^reg_val;
                self.set_szp(a_val^reg_val);
                self.registers.Flags&=0b11101110
            },
            (1,1,0)=>{
                self.registers.A=a_val|reg_val;
                self.set_szp(a_val|reg_val);
                self.registers.Flags&=0b11101110
            },
            (1,1,1)=>{
                self.add_3_szapc(a_val,!reg_val,1);
                self.write_c(reg_val as u16 > a_val as u16); // overwrite c
            },
            _ => {}
        }
    }
    fn rcc(&mut self,c2:u8,c1:u8,c0:u8) {
        self.ticks += 5;
        let condition:bool;
        match (c2,c1,c0) {
            (0,0,0)=>condition = !self.get_z(),
            (0,0,1)=>condition = self.get_z(),
            (0,1,0)=>condition = !self.get_c(),
            (0,1,1)=>condition = self.get_c(),
            (1,0,0)=>condition = self.get_p()==false,
            (1,0,1)=>condition = self.get_p()==true,
            (1,1,0)=>condition = self.get_p(),
            (1,1,1)=>condition = self.get_s()==true,
            _ => {condition=false;}
        }
        if condition {
            self.ticks += 6;
            self.PC=(self.memory[self.SP as usize] as u16) + ((self.memory[(self.SP+1) as usize]as u16)<<8 );
            self.SP = self.SP.wrapping_add(2);
        }
    }
    fn pop(&mut self,r1:u8,r0:u8) {
        self.ticks += 10;
        let cur_sp_hi = self.memory[(self.SP+1) as usize];
        let cur_sp_lo = self.memory[self.SP as usize];
        match self.rp {
            0=>{
                self.registers.B = cur_sp_hi;
                self.registers.C = cur_sp_lo;
            }
            1=>{
                self.registers.D = cur_sp_hi;
                self.registers.E = cur_sp_lo;
            }
            2=>{
                self.registers.H = cur_sp_hi;
                self.registers.L = cur_sp_lo;
            }
            3=>{
                self.registers.A = cur_sp_hi;
                self.registers.Flags = cur_sp_lo & 0b11010111 | 0b00000010;
            }
            _ => {
                return;
            }
        }
        self.SP = self.SP.wrapping_add(2);
    }
    fn jcc(&mut self,c2:u8,c1:u8,c0:u8) {
        self.ticks += 10;
        let addlo = self.read_next_byte();
        let addhi = self.read_next_byte();

        let condition:bool;
        match (c2,c1,c0) {
            (0,0,0)=>condition = !self.get_z(),
            (0,0,1)=>condition = self.get_z(),
            (0,1,0)=>condition = !self.get_c(),
            (0,1,1)=>condition = self.get_c(),
            (1,0,0)=>condition = self.get_p()==false,
            (1,0,1)=>condition = self.get_p()==true,
            (1,1,0)=>condition = self.get_p(),
            (1,1,1)=>condition = self.get_s()==true,
            _ => {condition=false;}
        }
        if condition {
            self.PC = (((addhi as usize) << 8) + (addlo as usize)) as u16;
        }
    }
    fn jmp(&mut self) {
        self.ticks += 10;
        let addlo = self.read_next_byte();
        let addhi = self.read_next_byte();
        self.PC = (((addhi as usize) << 8) + (addlo as usize)) as u16;
    }
    fn ccc(&mut self,c2:u8,c1:u8,c0:u8) {
        self.ticks += 11;
        let addlo = self.read_next_byte();
        let addhi = self.read_next_byte();

        let condition:bool;
        match (c2,c1,c0) {
            (0,0,0)=>condition = !self.get_z(),
            (0,0,1)=>condition = self.get_z(),
            (0,1,0)=>condition = !self.get_c(),
            (0,1,1)=>condition = self.get_c(),
            (1,0,0)=>condition = self.get_p()==false,
            (1,0,1)=>condition = self.get_p()==true,
            (1,1,0)=>condition = self.get_p(),
            (1,1,1)=>condition = self.get_s()==true,
            _ => {condition=false}
        }
        if condition {
            self.ticks += 6;
            self.SP-=2;
            self.memory[(self.SP) as usize]= (self.PC & 0x00FF) as u8;
            self.memory[(self.SP+1) as usize]= ((self.PC & 0xFF00) >> 8) as u8;
            self.PC = ((addhi as u16)<<8) + addlo as u16;
        }
    }
    fn push(&mut self,r1:u8,r0:u8) {
        self.ticks += 11;
        self.SP-=2;
        match self.rp {
            0=>{
                self.memory[self.SP as usize] = self.registers.C;
                self.memory[(self.SP+1) as usize] = self.registers.B;
            }
            1=>{
                self.memory[self.SP as usize] = self.registers.E;
                self.memory[(self.SP+1) as usize] = self.registers.D;
            }
            2=>{
                self.memory[self.SP as usize] = self.registers.L;
                self.memory[(self.SP+1) as usize] = self.registers.H;
            }
            3=>{
                self.memory[self.SP as usize] = self.registers.Flags;
                self.memory[(self.SP+1) as usize] = self.registers.A;
            }
            _ => {
                return;
            }
        }
    }

    // TODO need fix
    fn aluop2(&mut self,alu2:u8,alu1:u8,alu0:u8) {
        self.ticks += 7;
        let a_val = self.registers.A;
        let data_val = self.read_next_byte();
        match (alu2,alu1,alu0) {
            (0,0,0)=>self.registers.A=self.add_szapc(a_val,data_val),
            (0,0,1)=>{
                self.registers.A=self.add_3_szapc(a_val,data_val,self.registers.Flags & 0x01);
            },
            (0,1,0)=>{
                self.registers.A=self.add_3_szapc(a_val,!data_val,1);
                self.write_c(data_val as u16 > a_val as u16); // overwrite c
            },
            (0,1,1)=>{
                // let sum = data_val.wrapping_add((self.registers.Flags & 0x01) as u8);
                let complement = (!data_val); // 2s complement
                let carry = self.registers.Flags & 0x01;
                // let flags = self.registers.Flags;
                self.registers.A=self.add_3_szapc(a_val,complement,if carry == 1 { 0 } else { 1 });
                // let mysum = self.add_3_szapc(a_val,complement,if carry == 1 { 0 } else { 1 });
                self.write_c(data_val as u16 + (carry as u16) > a_val as u16);
                // let myflags = self.registers.Flags;
                // self.registers.Flags = flags;
                
                
                // let c = self.registers.Flags & 0x01;
                // let r = a_val.wrapping_sub(data_val).wrapping_sub(c);
                // 
                // 
                // self.set_szp(r);
                // self.write_a((a_val as i8 & 0x0f) - (data_val as i8 & 0x0f) - (c as i8) >= 0x00);
                // self.write_c(u16::from(a_val) < u16::from(data_val) + u16::from(c));
                // self.registers.A = r;
                // 
                // if (mysum!= r || myflags != self.registers.Flags) {
                //     println!("mysum {mysum} result {r} at a_val {a_val} and data_val {data_val} myflags {myflags} flags {}", self.registers.Flags);
                // }
            },
            (1,0,0)=>{
                // self.registers.A=a_val&data_val;self.set_szp(a_val&data_val);self.registers.Flags&=0b11101110; // use same behaviour as ANA

                self.set_szp(a_val&data_val);
                self.registers.Flags&=0b11101110;
                if (((self.registers.A | data_val) & 0x08) != 0){
                    self.registers.Flags |= 0b00010000;
                } else {
                    self.registers.Flags &= !0b00010000;
                }
                self.registers.A=a_val&data_val;
            },
            (1,0,1)=>{self.registers.A=a_val^data_val;self.set_szp(a_val^data_val);self.registers.Flags&=0b11101110},
            (1,1,0)=>{self.registers.A=a_val|data_val;self.set_szp(a_val|data_val);self.registers.Flags&=0b11101110},
            (1,1,1)=>{
                self.add_3_szapc(a_val,!data_val,1);
                self.write_c(data_val as u16 > a_val as u16); // overwrite c
                // let r = a_val.wrapping_sub(data_val);
                // self.set_szp(r);
                // self.write_a((a_val as i8 & 0x0f) - (data_val as i8 & 0x0f) >= 0x00);
                // self.write_c(u16::from(a_val) < u16::from(data_val));
            },
            _ => {}
        }
    }
    fn rst(&mut self,n2:u8,n1:u8,n0:u8) {
        self.ticks += 11;
        self.SP = self.SP.wrapping_sub(2);
        self.memory[self.SP as usize] = (self.PC & 0x00FF) as u8;
        self.memory[(self.SP+1) as usize] = ((self.PC & 0xFF00)>>8) as u8;
        self.PC=(((n2<<2)+(n1<<1)+n0) * 8)as u16;
    }
    fn ret(&mut self) {
        self.ticks += 10;
        self.PC=(self.memory[self.SP as usize] as u16) + ((self.memory[(self.SP+1) as usize]as u16)<<8 );
        self.SP=self.SP.wrapping_add(2);
    }
    fn call(&mut self) {
        self.ticks += 17;
        let addlo = self.read_next_byte();
        let addhi = self.read_next_byte();

        self.SP= self.SP.wrapping_sub(2);
        self.memory[self.SP as usize]= (self.PC & 0x00FF) as u8;
        self.memory[(self.SP+1) as usize]= ((self.PC & 0xFF00) >> 8) as u8;

        self.PC=((addhi as u16) << 8) + addlo as u16;

    }
    fn out_port(&mut self) {
        self.ticks += 10;
        let port = self.read_next_byte();
        self.oport.push((port,self.registers.A));
    }
    fn in_port(&mut self) {
        self.ticks += 10;
        let port = self.read_next_byte();
        // println!("Port: {} data: {} pc: {}",port,self.iport[port as usize],self.PC);
        self.registers.A = self.iport[port as usize];
    }
    fn xthl(&mut self) {
        self.ticks += 18;
        let temph = self.registers.H;
        let templ = self.registers.L;
        self.registers.L=self.memory[self.SP as usize];
        self.registers.H=self.memory[(self.SP+1) as usize];
        self.memory[self.SP as usize] = templ;
        self.memory[(self.SP+1) as usize] = temph;
    }
    fn pchl(&mut self) {
        self.ticks += 5;
        self.PC =((self.registers.H as u16) << 8) + self.registers.L as u16;
    }
    fn xchg(&mut self) {
        self.ticks += 4;
        let templ = self.registers.L;
        let temph = self.registers.H;
        self.registers.H=self.registers.D;
        self.registers.L=self.registers.E;
        self.registers.D=temph;
        self.registers.E=templ;
    }
    fn di(&mut self) {
        self.ticks += 4;
        self.interrupt_enabled = false;
    }
    fn sphl(&mut self) {
        self.ticks += 5;
        self.SP = ((self.registers.H as u16) << 8) + self.registers.L as u16;
    }
    fn ei(&mut self) {
        self.ticks += 4;
        self.interrupt_enabled = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const Init_Flag:u8 = 0b00000010;

    fn load_program(intel8080: & mut Intel8080, prog: Vec<u8>){
        intel8080.load_program(prog);
    }

    #[track_caller]
    fn compare_registers(intel8080: & Intel8080,a:u8,f:u8,b:u8,c:u8,d:u8,e:u8,h:u8,l:u8){
        assert_eq!(intel8080.registers.A, a, "The expected result in register A is {} but got {}",a,intel8080.registers.A);
        assert_eq!(intel8080.registers.Flags, f,"The expected result in register Flags is {} but got {}",f,intel8080.registers.Flags);
        assert_eq!(intel8080.registers.C, c,"The expected result in register C is {} but got {}",c,intel8080.registers.C);
        assert_eq!(intel8080.registers.D, d,"The expected result in register D is {} but got {}",d,intel8080.registers.D);
        assert_eq!(intel8080.registers.E, e,"The expected result in register E is {} but got {}",e,intel8080.registers.E);
        assert_eq!(intel8080.registers.H, h,"The expected result in register H is {} but got {}",h,intel8080.registers.H);
        assert_eq!(intel8080.registers.L, l,"The expected result in register L is {} but got {}",l,intel8080.registers.L);
    }

    #[track_caller]
    fn compare_memory(intel8080: & Intel8080,loc:u16, value:u8){
        assert_eq!(intel8080.memory[loc as usize], value, "Expected value at memory location {} is {} but got {}", loc, value, intel8080.memory[loc as usize]);
    }

    #[test]
    fn init(){
        let intel8080 = Intel8080::new();
        assert!(intel8080.SP==0);
    }
    #[test]
    fn nop(){
        let mut intel8080 = Intel8080::new();
        load_program(&mut intel8080,vec![0]);
        intel8080.cycle();
        compare_registers(&intel8080, 0, Init_Flag, 0, 0, 0, 0, 0, 0);
    }
    #[test]
    // [0,0,r1,r0,0,0,0,1] datlo dathi
    fn lxi(){
        // register pair bc
        let mut i0 = Intel8080::new();
        load_program(&mut i0,vec![1,0xFF,1]);
        i0.cycle();
        compare_registers(&i0, 0, Init_Flag, 1, 0xFF, 0, 0, 0, 0);

        // register pair de
        let mut i1 = Intel8080::new();
        load_program(&mut i1,vec![0b00010001,0xFF,1]);
        i1.cycle();
        compare_registers(&i1, 0, Init_Flag, 0, 0, 1, 0xFF, 0, 0);

        // register pair hl        i0.cycle();

        let mut i2 = Intel8080::new();
        load_program(&mut i2,vec![0b00100001,0xFF,1]);
        i2.cycle();
        compare_registers(&i2, 0, Init_Flag, 0, 0, 0, 0, 1, 0xFF);
    }

    #[test]
    // [0,0,r1,r0,0,0,1,0]
    fn stax(){
        // register pair bc
        let mut i0 = Intel8080::new();
        //                       LXI BC, 0x12ab, MVI A, 0xFF, STAX BC
        load_program(&mut i0, vec![1, 0xab, 0x12, 0b00111110, 0xFF, 0b00000010]);
        i0.cycle();
        i0.cycle();
        i0.cycle();
        compare_memory(&i0,0x12ab,0xff);

        // register pair de
        let mut i0 = Intel8080::new();
        //                       LXI DE, 0x12ab, MVI A, 0xFF, STAX BC
        load_program(&mut i0, vec![0b00010001, 0xab, 0x12, 0b00111110, 0xFF, 0b00010010]);
        i0.cycle();
        i0.cycle();
        i0.cycle();
        compare_memory(&i0,0x12ab,0xff);
    }

    #[test]
    // [0,0,r1,r0,0,0,1,1]
    fn inx(){
        // register pair bc
        let mut i0 = Intel8080::new();
        //                       LXI BC, 0x12ab, INX BC
        load_program(&mut i0,vec![1,0xab,0x12,3]);
        i0.cycle();
        i0.cycle();
        compare_registers(&i0, 0, Init_Flag, 0x12, 0xac, 0, 0, 0, 0);

        // register pair de
        let mut i1 = Intel8080::new();
        //                       LXI DE, 0x12ab, INX DE
        load_program(&mut i1, vec![0b00010001, 0xab, 0x12, 0b00010011]);
        i1.cycle();
        i1.cycle();
        compare_registers(&i1, 0, Init_Flag, 0, 0, 0x12, 0xac, 0, 0);

        // register pair hl
        let mut i2 = Intel8080::new();
        //                       LXI HL, 0x12ab, INX HL
        load_program(&mut i2, vec![0b00100001, 0xab, 0x12, 0b00100011]);
        i2.cycle();
        i2.cycle();
        compare_registers(&i2, 0, Init_Flag, 0, 0, 0, 0, 0x12, 0xac);

        // register pair sp
        let mut i2 = Intel8080::new();
        //                       LXI HL, 0x12ab, INX SP
        load_program(&mut i2, vec![0b00110001, 0xab, 0x12, 0b00110011]);
        i2.cycle();
        i2.cycle();
        assert_eq!(i2.get_sp(),0x12ac);
        compare_registers(&i2, 0, Init_Flag, 0, 0, 0, 0, 0, 0);
    }

    #[test]
    // [0,0,d2,d1,d0,1,0,0]
    fn inr(){
        // register b
        let mut i0 = Intel8080::new();
        //                       MVI B 0x1F       INR B
        load_program(&mut i0,vec![0b00000110,0x1F,0b00000100]);
        i0.cycle();
        i0.cycle();
        compare_registers(&i0,0,0b00010010,0x20, 0,0,0,0,0);

        // register c
        let mut i0 = Intel8080::new();
        //                       MVI C 0x1F       INR C
        load_program(&mut i0,vec![0b00001110,0x1F,0b00001100]);
        i0.cycle();
        i0.cycle();
        compare_registers(&i0,0,0b00010010,0, 0x20,0,0,0,0);

        // TODO More tests
    }

    #[test]
    // [0,0,d2,d1,d0,1,0,1]
    fn dcr(){
        // register b
        let mut i0 = Intel8080::new();
        //                             MVI B 0x1F       DCR B
        load_program(&mut i0,vec![0b00000110,0x1F,0b00000101]);
        i0.cycle();
        i0.cycle();
        compare_registers(&i0,0,0b00010110,0x1E, 0,0,0,0,0);

        // register c
        let mut i0 = Intel8080::new();
        //                             MVI C 0x1F       DCR C
        load_program(&mut i0,vec![0b00001110,0x1F,0b00001101]);
        i0.cycle();
        i0.cycle();
        compare_registers(&i0,0,0b00010110,0, 0x1E,0,0,0,0);

        // TODO More tests
    }

    #[test]
    // [0,0,d2,d1,d0,1,1,0] data
    fn mvi(){
        // register b
        let mut i0 = Intel8080::new();
        //                             MVI B 0x1F
        load_program(&mut i0,vec![0b00000110,0x1F]);
        i0.cycle();
        compare_registers(&i0, 0, Init_Flag, 0x1F, 0, 0, 0, 0, 0);

        // register c
        let mut i0 = Intel8080::new();
        //                             MVI C 0x1F
        load_program(&mut i0,vec![0b00001110,0x1F]);
        i0.cycle();
        compare_registers(&i0, 0, Init_Flag, 0, 0x1F, 0, 0, 0, 0);

        // TODO More tests
    }

    #[test]
    // [0,0,r1,r0,1,0,0,1]
    fn dad(){
        // register pair bc
        let mut i0 = Intel8080::new();
        //                          LXI BC, 0x12ab, LXI HL 0xFFFF, DAD BC
        load_program(&mut i0,vec![1,0xab,0x12,0b00100001,0xFF, 0xFF, 0b00001001]);
        i0.cycle();
        i0.cycle();
        i0.cycle();
        compare_registers(&i0,0,3,0x12, 0xab,0,0,0x12,0xaa);

        // register pair de
        let mut i0 = Intel8080::new();
        //                             LXI DE, 0x12ab, LXI HL 0xFFFF, DAD DE
        load_program(&mut i0,vec![0b00010001,0xab,0x12,0b00100001,0xFF, 0xFF, 0b00011001]);
        i0.cycle();
        i0.cycle();
        i0.cycle();
        compare_registers(&i0,0,3,0, 0,0x12,0xab,0x12,0xaa);

        // register pair hl
        let mut i0 = Intel8080::new();
        //                             LXI HL, 0x12ab, DAD HL
        load_program(&mut i0,vec![0b00100001,0xab,0x12,0b00101001]);
        i0.cycle();
        i0.cycle();
        compare_registers(&i0, 0, Init_Flag, 0, 0, 0, 0, 0x25, 0x56);

        // register pair sp
        let mut i0 = Intel8080::new();
        //                             LXI SP, 0x12ab, LXI HL 0xFFFF, DAD SP
        load_program(&mut i0,vec![0b00110001,0xab,0x12,0b00100001,0xFF, 0xFF, 0b00111001]);
        i0.cycle();
        i0.cycle();
        i0.cycle();
        assert_eq!(i0.get_sp(),0x12ab);
        compare_registers(&i0,0,3,0, 0,0,0,0x12,0xaa);
    }

    #[test]
    // [0,0,r1,r0,1,0,1,0]
    fn ldax(){
        // register pair bc
        let mut i0 = Intel8080::new();
        //                          LXI BC, 0x0110, LDAX BC
        load_program(&mut i0,vec![1,0x10,0x01,0b00001010]);
        i0.memory[0x0110] = 0xda;
        i0.cycle();
        i0.cycle();
        compare_registers(&i0, 0xda, Init_Flag, 0x01, 0x10, 0, 0, 0, 0);

        // register pair de
        let mut i0 = Intel8080::new();
        //                          LXI DE, 0x0110, LDAX DE
        load_program(&mut i0,vec![0b00010001,0x10,0x01,0b00011010]);
        i0.memory[0x0110] = 0xdb;
        i0.cycle();
        i0.cycle();
        compare_registers(&i0, 0xdb, Init_Flag, 0, 0, 0x01, 0x10, 0, 0);
    }

    #[test]
    // [0,0,r1,r0,1,0,1,1]
    fn dcx(){
        // register pair bc
        let mut i0 = Intel8080::new();
        //                       LXI BC, 0x12ab, DCX BC
        load_program(&mut i0,vec![1,0xab,0x12,0b00001011]);
        i0.cycle();
        i0.cycle();
        compare_registers(&i0, 0, Init_Flag, 0x12, 0xaa, 0, 0, 0, 0);

        // register pair de
        let mut i1 = Intel8080::new();
        //                       LXI DE, 0x12ab, DCX DE
        load_program(&mut i1, vec![0b00010001, 0xab, 0x12, 0b00011011]);
        i1.cycle();
        i1.cycle();
        compare_registers(&i1, 0, Init_Flag, 0, 0, 0x12, 0xaa, 0, 0);

        // register pair hl
        let mut i2 = Intel8080::new();
        //                       LXI HL, 0x12ab, DCX HL
        load_program(&mut i2, vec![0b00100001, 0xab, 0x12, 0b00101011]);
        i2.cycle();
        i2.cycle();
        compare_registers(&i2, 0, Init_Flag, 0, 0, 0, 0, 0x12, 0xaa);

        // register pair sp
        let mut i2 = Intel8080::new();
        //                       LXI HL, 0x12ab, DCX SP
        load_program(&mut i2, vec![0b00110001, 0xab, 0x12, 0b00111011]);
        i2.cycle();
        i2.cycle();
        assert_eq!(i2.get_sp(),0x12aa);
        compare_registers(&i2, 0, Init_Flag, 0, 0, 0, 0, 0, 0);
    }

    #[test]
    // [0,0,0,0,0,1,1,1]
    fn rlc(){
        // carry
        let mut i0 = Intel8080::new();
        //                             MVI A 0b10110111, RLC
        load_program(&mut i0,vec![0b00111110,0b10110111,0b00000111]);
        i0.cycle();
        i0.cycle();
        compare_registers(&i0, 0b01101111, Init_Flag |(1), 0, 0, 0, 0, 0, 0);

        // no carry
        let mut i0 = Intel8080::new();
        //                             MVI A 0b01011011, RLC
        load_program(&mut i0,vec![0b00111110,0b01011011,0b00000111]);
        i0.cycle();
        i0.cycle();
        compare_registers(&i0, 0b10110110, Init_Flag, 0, 0, 0, 0, 0, 0);
    }

    #[test]
    // [0,0,0,0,1,1,1,1]
    fn rrc(){
        // carry
        let mut i0 = Intel8080::new();
        //                             MVI A 0b10110111, RRC
        load_program(&mut i0,vec![0b00111110,0b10110111,0b00001111]);
        i0.cycle();
        i0.cycle();
        compare_registers(&i0, 0b11011011, Init_Flag |(1), 0, 0, 0, 0, 0, 0);

        // no carry
        let mut i0 = Intel8080::new();
        //                             MVI A 0b11011010, RRC
        load_program(&mut i0,vec![0b00111110,0b11011010,0b00001111]);
        i0.cycle();
        i0.cycle();
        compare_registers(&i0, 0b01101101, Init_Flag, 0, 0, 0, 0, 0, 0);
    }

    #[test]
    // [0,0,0,1,0,1,1,1]
    fn ral(){
        // carry
        let mut i0 = Intel8080::new();
        //                             MVI A 0xb10110111, RAL
        load_program(&mut i0,vec![0b00111110,0b10110111,0b00010111]);
        i0.cycle();
        i0.cycle();
        compare_registers(&i0, 0b01101110, Init_Flag |(1), 0, 0, 0, 0, 0, 0);

        // no carry
        let mut i0 = Intel8080::new();
        //                             MVI A 0b01011011, RAL
        load_program(&mut i0,vec![0b00111110,0b01011011,0b00010111]);
        i0.cycle();
        i0.cycle();
        compare_registers(&i0, 0b10110110, Init_Flag, 0, 0, 0, 0, 0, 0);
    }

    #[test]
    // [0,0,0,1,1,1,1,1]
    fn rar(){
        // carry
        let mut i0 = Intel8080::new();
        //                             MVI A 0xb10110111, RAR
        load_program(&mut i0,vec![0b00111110,0b10110111,0b00011111]);
        i0.cycle();
        i0.cycle();
        compare_registers(&i0, 0b01011011, Init_Flag |(1), 0, 0, 0, 0, 0, 0);

        // no carry
        let mut i0 = Intel8080::new();
        //                             MVI A 0b01011011, RAL
        load_program(&mut i0,vec![0b00111110,0b01011010,0b00011111]);
        i0.cycle();
        i0.cycle();
        compare_registers(&i0, 0b00101101, Init_Flag, 0, 0, 0, 0, 0, 0);
    }

    #[test]
    // [0,0,1,0,0,0,1,0]
    fn shld(){
        let mut i0 = Intel8080::new();
        //                             LXI HL 0x12ab, SHLD 0x111
        load_program(&mut i0,vec![0b00100001,0xab, 0x12,0b00100010, 0x11, 0x01]);
        i0.cycle();
        i0.cycle();
        compare_memory(&i0,0x0111,0xab);
        compare_memory(&i0,0x0112,0x12);
        compare_registers(&i0, 0, Init_Flag, 0, 0, 0, 0, 0x12, 0xab);
    }

    #[test]
    // [0,0,1,0,0,1,1,1]
    fn daa(){
        let mut i0 = Intel8080::new();
        //                             MVI A 0b00011111, DAA
        load_program(&mut i0,vec![0b00111110,0b10011011,0b00100111]);
        i0.cycle();
        i0.cycle();
        compare_registers(&i0,0b00000001,0b00010011,0, 0,0,0,0,0);
    }

    #[test]
    // [0,0,1,0,0,1,1,1]
    fn daa2(){
        let mut i0 = Intel8080::new();
        //                             MVI A 0x9b,      DAA
        load_program(&mut i0,vec![0b00111110,0x9b,0b00100111]);
        i0.cycle();
        i0.cycle();
        compare_registers(&i0,0b00000001,0b00010011,0, 0,0,0,0,0);
    }

    #[test]
    // [0,0,1,0,1,0,1,0] addlo addhi
    fn lhld(){
        let mut i0 = Intel8080::new();
        //                             LHLD 0x0003, 0xbb, 0xaa
        load_program(&mut i0,vec![0b00101010,0x03,0x00,0xaa,0xbb]);
        i0.cycle();
        compare_registers(&i0, 0, Init_Flag, 0, 0, 0, 0, 0xbb, 0xaa);
    }

    #[test]
    // [0,0,1,0,1,1,1,1]
    fn cma(){
        let mut i0 = Intel8080::new();
        //                             MVI A 0xFF, CMA
        load_program(&mut i0,vec![0b00111110,0xFF,0b00101111]);
        i0.cycle();
        i0.cycle();
        compare_registers(&i0, 0, Init_Flag, 0, 0, 0, 0, 0, 0);
    }

    #[test]
    // [0,0,1,1,0,0,1,0] addlo addhi
    fn sta(){
        let mut i0 = Intel8080::new();
        //                             MVI A 0xFF,     STA 0xabcd
        load_program(&mut i0,vec![0b00111110,0xFF,0b00110010, 0xcd, 0xab]);
        i0.cycle();
        i0.cycle();
        compare_memory(&i0, 0xabcd, 0xFF);
        compare_registers(&i0, 0xFF, Init_Flag, 0, 0, 0, 0, 0, 0);
    }

    #[test]
    // [0,0,1,1,0,1,1,1]
    fn stc(){
        let mut i0 = Intel8080::new();
        //                             STC
        load_program(&mut i0,vec![0b00110111]);
        i0.cycle();
        compare_registers(&i0, 0, Init_Flag | 1, 0, 0, 0, 0, 0, 0);
    }

    #[test]
    // [0,0,1,1,1,0,1,0] addlo addhi
    fn lda(){
        let mut i0 = Intel8080::new();
        //                             LDA 0x0003
        load_program(&mut i0,vec![0b00111010, 0x03, 0x00, 0xab]);
        i0.cycle();
        compare_registers(&i0, 0xab, Init_Flag, 0, 0, 0, 0, 0, 0);
    }

    #[test]
    // [0,0,1,1,1,1,1,1]
    fn cmc(){
        let mut i0 = Intel8080::new();
        //                             CMC
        load_program(&mut i0,vec![0b00111111]);
        i0.cycle();
        compare_registers(&i0, 0, Init_Flag | 1, 0, 0, 0, 0, 0, 0);

        let mut i0 = Intel8080::new();
        //                             STC         CMC
        load_program(&mut i0,vec![0b00110111, 0b00111111]);
        i0.cycle();
        i0.cycle();
        compare_registers(&i0, 0, Init_Flag & !1, 0, 0, 0, 0, 0, 0);
    }

    #[test]
    // [0,1,d2,d1,d0,s2,s1,s0]
    fn mov(){
        let mut i0 = Intel8080::new();
        //                             MVI A 0xFF      MOV B A
        load_program(&mut i0,vec![0b00111110,0xFF,0b01000111]);
        i0.cycle();
        i0.cycle();
        compare_registers(&i0, 0xFF, Init_Flag, 0xFF, 0, 0, 0, 0, 0);

        let mut i0 = Intel8080::new();
        //                             MVI A 0xFF        LXI HL 0x0006        MOV M A
        load_program(&mut i0,vec![0b00111110,0xFF, 0b00100001,0x06,0x00,0b01110111]);
        i0.cycle();
        i0.cycle();
        i0.cycle();
        compare_memory(&i0,0x0006, 0xFF);
        compare_registers(&i0, 0xFF, Init_Flag, 0, 0, 0, 0, 0, 0x06);

        let mut i0 = Intel8080::new();
        //                             MVI A 0xFF        LXI HL 0x0006        MOV A M
        load_program(&mut i0,vec![0b00111110,0xFF, 0b00100001,0x06,0x00,0b01111110, 0xab]);
        i0.cycle();
        i0.cycle();
        i0.cycle();
        compare_registers(&i0, 0xab, Init_Flag, 0, 0, 0, 0, 0, 0x06);
    }

    #[test]
    // [1,0,alu2,alu1,alu0,s2,s1,s0]
    fn alu1(){
        // ADD
        let mut i0 = Intel8080::new();
        //                             MVI A 0xFF,     MVI B 0x02,       ADD B
        load_program(&mut i0,vec![0b00111110,0xFF,0b00000110, 0x02, 0b10000000]);
        i0.cycle();
        i0.cycle();
        i0.cycle();
        compare_registers(&i0,0x01,0b00010011,0x02, 0,0,0,0,0);

        // ADC
        let mut i0 = Intel8080::new();
        //                             MVI A 0xFF,     MVI B 0x02,        ADD B,     ADC B
        load_program(&mut i0,vec![0b00111110,0xFF,0b00000110, 0x02, 0b10000000, 0b10001000]);
        i0.cycle();
        i0.cycle();
        i0.cycle();
        i0.cycle();
        compare_registers(&i0,0x04,0b00000010,0x02, 0,0,0,0,0);

        // SUB
        let mut i0 = Intel8080::new();
        //                             MVI A 0x00,     MVI B 0x02,        SUB B
        load_program(&mut i0,vec![0b00111110,0x00,0b00000110, 0x02, 0b10010000]);
        i0.cycle();
        i0.cycle();
        i0.cycle();
        compare_registers(&i0,0xFE,0b10000011,0x02, 0,0,0,0,0);

        // SBB
        let mut i0 = Intel8080::new();
        //                             MVI A 0x00,     MVI B 0x02,        SUB B      SBB B
        load_program(&mut i0,vec![0b00111110,0x00,0b00000110, 0x02, 0b10010000, 0b10011000]);
        i0.cycle();
        i0.cycle();
        i0.cycle();
        i0.cycle();
        compare_registers(&i0,0xFB,0b10010010,0x02, 0,0,0,0,0);

        // ANA
        let mut i0 = Intel8080::new();
        //                             MVI A 0xFF,     MVI B 0x02,        ANA B
        load_program(&mut i0,vec![0b00111110,0xFF,0b00000110, 0x02, 0b10100000]);
        i0.cycle();
        i0.cycle();
        i0.cycle();
        compare_registers(&i0,0x02,0b00010010,0x02, 0,0,0,0,0);

        // XRA
        let mut i0 = Intel8080::new();
        //                             MVI A 0xFF,     MVI B 0x02,        XRA B
        load_program(&mut i0,vec![0b00111110,0xFF,0b00000110, 0x02, 0b10101000]);
        i0.cycle();
        i0.cycle();
        i0.cycle();
        compare_registers(&i0,0b11111101,0b10000010,0x02, 0,0,0,0,0);

        // ORA
        let mut i0 = Intel8080::new();
        //                             MVI A 0b11110000,     MVI B 0x02,        ORA B
        load_program(&mut i0,vec![0b00111110,0b11110000,0b00000110, 0b00110011, 0b10110000]);
        i0.cycle();
        i0.cycle();
        i0.cycle();
        compare_registers(&i0,0b11110011,0b10000110,0b00110011, 0,0,0,0,0);

        // CMP
        let mut i0 = Intel8080::new();
        //                             MVI A 0x00,     MVI B 0x02,        CMP B
        load_program(&mut i0,vec![0b00111110,0x00,0b00000110, 0x02, 0b10111000]);
        i0.cycle();
        i0.cycle();
        i0.cycle();
        compare_registers(&i0,0x00,0b10000011,0x02, 0,0,0,0,0);
    }

    #[test]
    // [1,1,cc2,cc1,cc0,0,0,0]
    fn rcc() {
        // C
        let mut i0 = Intel8080::new();
        //                             MVI A 0xFF,        MVI B 0x02,       ADD B,      LXI SP 0xabcd,          RCC C
        load_program(&mut i0, vec![0b00111110, 0xFF, 0b00000110, 0x02, 0b10000000, 0b00110001, 0xcd, 0xab, 0b11011000]);
        i0.memory[0xabcd] = 0xda;
        i0.memory[0xabcd+1] = 0xcb;
        i0.cycle();
        i0.cycle();
        i0.cycle();
        i0.cycle();
        i0.cycle();
        assert_eq!(i0.SP,0xabcd+2);
        assert_eq!(i0.PC,0xcbda);
        compare_registers(&i0, 0x01, 0b00010011, 0x02, 0, 0, 0, 0, 0);
    }

    #[test]
    // [1,1,r1,r0,0,0,0,1]
    fn pop() {
        // C
        let mut i0 = Intel8080::new();
        //                              LXI SP 0xabcd,          POP bc
        load_program(&mut i0, vec![0b00110001, 0xcd, 0xab, 0b11000001]);
        i0.memory[0xabcd] = 0xda;
        i0.memory[0xabcd+1] = 0xcb;
        i0.cycle();
        i0.cycle();
        assert_eq!(i0.SP,0xabcd+2);
        compare_registers(&i0, 0, 0b00000010, 0xcb, 0xda, 0, 0, 0, 0);
    }

    #[test]
    // [1,1,cc2,cc1,cc0,0,1,0]
    fn jcc() {
        // C
        let mut i0 = Intel8080::new();
        //                             MVI A 0xFF,        MVI B 0x02,       ADD B,     JCC C
        load_program(&mut i0, vec![0b00111110, 0xFF, 0b00000110, 0x02, 0b10000000, 0b11011010,0xcd, 0xab]);
        i0.cycle();
        i0.cycle();
        i0.cycle();
        i0.cycle();
        assert_eq!(i0.PC,0xabcd);
        compare_registers(&i0, 0x01, 0b00010011, 0x02, 0, 0, 0, 0, 0);
    }

    #[test]
    // [1,1,cc2,cc1,cc0,1,0,0]
    fn ccc() {
        // C
        let mut i0 = Intel8080::new();
        //                             MVI A 0xFF,        MVI B 0x02,       ADD B,      LXI SP 0xabcd,          CCC C
        load_program(&mut i0, vec![0b00111110, 0xFF, 0b00000110, 0x02, 0b10000000, 0b00110001, 0xcd, 0xab, 0b11011100,0xea, 0xfc]);
        i0.cycle();
        i0.cycle();
        i0.cycle();
        i0.cycle();
        i0.cycle();
        assert_eq!(i0.PC,0xfcea);
        assert_eq!(i0.SP,0xabcd-2);
        compare_registers(&i0, 0x01, 0b00010011, 0x02, 0, 0, 0, 0, 0);
    }

    #[test]
    // [1,1,r1,r0,0,1,0,1]
    fn push() {
        let mut i0 = Intel8080::new();
        //                              LXI SP 0xabcd,          LXI bc 0xbeef           PUSH bc
        load_program(&mut i0, vec![0b00110001, 0xcd, 0xab, 0b00000001, 0xef, 0xbe, 0b11000101]);
        i0.memory[0xabcd] = 0xda;
        i0.memory[0xabcd+1] = 0xcb;
        i0.cycle();
        i0.cycle();
        i0.cycle();
        assert_eq!(i0.SP,0xabcd-2);
        compare_memory(&i0,0xabcd-2,0xef);
        compare_memory(&i0,0xabcd-1,0xbe);
        compare_registers(&i0, 0, 0b00000010, 0xbe, 0xef, 0, 0, 0, 0);
    }

    #[test]
    // [1,1,alu2,alu1,alu0,1,1,0] data
    fn alu2(){
        // ADD
        let mut i0 = Intel8080::new();
        //                             MVI A 0xFF,     MVI B 0x02,       ADD 0x02
        load_program(&mut i0,vec![0b00111110,0xFF,0b00000110, 0x02, 0b11000110,0x02]);
        i0.cycle();
        i0.cycle();
        i0.cycle();
        compare_registers(&i0,0x01,0b00010011,0x02, 0,0,0,0,0);

        // ADC
        let mut i0 = Intel8080::new();
        //                             MVI A 0xFF,     MVI B 0x02,        ADD B,     ADC 0x02
        load_program(&mut i0,vec![0b00111110,0xFF,0b00000110, 0x02, 0b10000000, 0b11001110, 0x02]);
        i0.cycle();
        i0.cycle();
        i0.cycle();
        i0.cycle();
        compare_registers(&i0,0x04,0b00000010,0x02, 0,0,0,0,0);

        // SUB
        let mut i0 = Intel8080::new();
        //                             MVI A 0x00,     MVI B 0x02,        SUB B
        load_program(&mut i0,vec![0b00111110,0x00,0b00000110, 0x02, 0b11010110, 0x02]);
        i0.cycle();
        i0.cycle();
        i0.cycle();
        compare_registers(&i0,0xFE,0b10000011,0x02, 0,0,0,0,0);

        // SBB
        let mut i0 = Intel8080::new();
        //                             MVI A 0x00,     MVI B 0x02,        SUB B      SBB 0x02
        load_program(&mut i0,vec![0b00111110,0x00,0b00000110, 0x02, 0b10010000, 0b11011110, 0x02]);
        i0.cycle();
        i0.cycle();
        i0.cycle();
        i0.cycle();
        compare_registers(&i0,0xFB,0b10010011,0x02, 0,0,0,0,0);

        // ANA
        let mut i0 = Intel8080::new();
        //                             MVI A 0xFF,     MVI B 0x02,        ANA B
        load_program(&mut i0,vec![0b00111110,0xFF,0b00000110, 0x02, 0b11100110, 0x02]);
        i0.cycle();
        i0.cycle();
        i0.cycle();
        compare_registers(&i0,0x02,0b00000010,0x02, 0,0,0,0,0);

        // XRA
        let mut i0 = Intel8080::new();
        //                             MVI A 0xFF,     MVI B 0x02,        XRA B
        load_program(&mut i0,vec![0b00111110,0xFF,0b00000110, 0x02, 0b11101110, 0x02]);
        i0.cycle();
        i0.cycle();
        i0.cycle();
        compare_registers(&i0,0b11111101,0b10000010,0x02, 0,0,0,0,0);

        // ORA
        let mut i0 = Intel8080::new();
        //                             MVI A 0b11110000,     MVI B 0x02,             ORA 0b00110011
        load_program(&mut i0,vec![0b00111110,0b11110000,0b00000110, 0b00110011, 0b10110000, 0b00110011]);
        i0.cycle();
        i0.cycle();
        i0.cycle();
        compare_registers(&i0,0b11110011,0b10000110,0b00110011, 0,0,0,0,0);

        // CMP
        let mut i0 = Intel8080::new();
        //                             MVI A 0x00,     MVI B 0x02,        CMP B
        load_program(&mut i0,vec![0b00111110,0x00,0b00000110, 0x02, 0b11111110, 0x02]);
        i0.cycle();
        i0.cycle();
        i0.cycle();
        compare_registers(&i0,0x00,0b10000011,0x02, 0,0,0,0,0);
    }

    #[test]
    // [1,1,n2,n1,n0,1,1,1]
    fn rst() {
        // C
        let mut i0 = Intel8080::new();
        //                              LXI SP 0xabcd,          RST 1
        load_program(&mut i0, vec![0b00110001, 0xcd, 0xab, 0b11001111]);
        i0.cycle();
        i0.cycle();
        assert_eq!(i0.PC,0x0008);
        assert_eq!(i0.SP,0xabcd-2);
        compare_memory(&i0,0xabcd-2,0x04);
        compare_memory(&i0,0xabcd-1,0x00);
        compare_registers(&i0, 0, Init_Flag, 0, 0, 0, 0, 0, 0);
    }

    #[test]
    // [1,1,0,0,1,0,0,1]
    fn ret() {
        let mut i0 = Intel8080::new();
        //                              LXI SP 0xabcd,          RET
        load_program(&mut i0, vec![0b00110001, 0xcd, 0xab, 0b11001001]);
        i0.memory[0xabcd] = 0xda;
        i0.memory[0xabcd+1] = 0xcb;
        i0.cycle();
        i0.cycle();
        assert_eq!(i0.PC,0xcbda);
        assert_eq!(i0.SP,0xabcd+2);
        compare_registers(&i0, 0, Init_Flag, 0, 0, 0, 0, 0, 0);
    }

    #[test]
    // [1,1,0,0,1,1,0,1] addlo addhi
    fn call() {
        let mut i0 = Intel8080::new();
        //                              LXI SP 0xabcd,          CALL 0xbeef
        load_program(&mut i0, vec![0b00110001, 0xcd, 0xab, 0b11001101, 0xef, 0xbe]);
        i0.memory[0xabcd] = 0xda;
        i0.memory[0xabcd+1] = 0xcb;
        i0.cycle();
        i0.cycle();
        assert_eq!(i0.PC,0xbeef);
        assert_eq!(i0.SP,0xabcd-2);
        compare_memory(&i0,0xabcd-2,0x06);
        compare_memory(&i0,0xabcd-1,0x00);
        compare_registers(&i0, 0, Init_Flag, 0, 0, 0, 0, 0, 0);
    }

    // #[test]
    // // [1,1,0,1,0,0,1,1] port
    // fn out_port() {
    //     let mut i0 = Intel8080::new();
    //     //                              MVI A 0xde,      OUT 0xba
    //     load_program(&mut i0, vec![0b00111110,0xde, 0b11010011, 0xba]);
    //     i0.cycle();
    //     i0.cycle();
    //     assert_eq!(i0.oport[0xba],0xde);
    //     compare_registers(&i0, 0xde, Init_Flag, 0, 0, 0, 0, 0, 0);
    // }
    // 
    // #[test]
    // // [1,1,0,1,1,0,1,1] port
    // fn in_port() {
    //     let mut i0 = Intel8080::new();
    //     //                         OUT 0xba
    //     load_program(&mut i0, vec![0b11011011, 0xba]);
    //     i0.iport[0xba] = 0xde;
    //     i0.cycle();
    //     i0.cycle();
    //     compare_registers(&i0, 0xde, Init_Flag, 0, 0, 0, 0, 0, 0);
    // }

    #[test]
    // [1,1,1,0,0,0,1,1]
    fn xthl() {
        let mut i0 = Intel8080::new();
        //                              LXI SP 0xabcd,          LXI HL 0xbeef,          XTHL
        load_program(&mut i0, vec![0b00110001, 0xcd, 0xab, 0b00100001, 0xef, 0xbe, 0b11100011]);
        i0.memory[0xabcd] = 0xda;
        i0.memory[0xabcd+1] = 0xcb;
        i0.cycle();
        i0.cycle();
        i0.cycle();
        compare_memory(&i0,0xabcd,0xef);
        compare_memory(&i0,0xabcd+1,0xbe);
        compare_registers(&i0, 0, Init_Flag, 0, 0, 0, 0, 0xcb, 0xda);
    }

    #[test]
    // [1,1,1,0,1,0,0,1]
    fn pchl() {
        let mut i0 = Intel8080::new();
        //                              LXI HL 0xbeef,          PCHL
        load_program(&mut i0, vec![0b00100001, 0xef, 0xbe, 0b11101001]);
        i0.cycle();
        i0.cycle();
        assert_eq!(i0.PC,0xbeef);
        compare_registers(&i0, 0, Init_Flag, 0, 0, 0, 0, 0xbe, 0xef);
    }

    #[test]
    // [1,1,1,0,1,0,1,1]
    fn xchg() {
        let mut i0 = Intel8080::new();
        //                              LXI DE 0xabcd,          LXI HL 0xbeef,          XCHG
        load_program(&mut i0, vec![0b00010001, 0xcd, 0xab, 0b00100001, 0xef, 0xbe, 0b11101011]);
        i0.cycle();
        i0.cycle();
        i0.cycle();
        compare_registers(&i0, 0, Init_Flag, 0, 0, 0xbe, 0xef, 0xab, 0xcd);
    }

    #[test]
    // [1,1,1,1,0,0,1,1]
    fn di() {
        let mut i0 = Intel8080::new();
        //                              DI
        load_program(&mut i0, vec![0b11110011]);
        i0.cycle();
        assert_eq!(i0.interrupt_enabled, false);
        compare_registers(&i0, 0, Init_Flag, 0, 0, 0, 0, 0, 0);
    }

    #[test]
    // [1,1,1,1,1,0,0,1]
    fn sphl() {
        let mut i0 = Intel8080::new();
        //                              LXI SP 0xabcd,          LXI HL 0xbeef,          SPHL
        load_program(&mut i0, vec![0b00110001, 0xcd, 0xab, 0b00100001, 0xef, 0xbe, 0b11111001]);
        i0.cycle();
        i0.cycle();
        i0.cycle();
        assert_eq!(i0.SP,0xbeef);
        compare_registers(&i0, 0, Init_Flag, 0, 0, 0, 0, 0xbe, 0xef);
    }

    #[test]
    // [1,1,1,1,1,0,1,1]
    fn ei() {
        let mut i0 = Intel8080::new();
        //                              DI
        load_program(&mut i0, vec![0b11110011, 0b11111011]);
        i0.cycle();
        assert_eq!(i0.interrupt_enabled, false);
        i0.cycle();
        assert_eq!(i0.interrupt_enabled, true);
        compare_registers(&i0, 0, Init_Flag, 0, 0, 0, 0, 0, 0);
    }
}
