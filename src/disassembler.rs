use std::io::Write;

pub struct Disassembler {
    buffer: Vec<u8>,
    index: usize,
    jump_table: Vec<u16>,
}

impl Disassembler {
    pub fn new() -> Self {
        Self { buffer: Vec::new(), index: 0, jump_table: vec![] }
    }

    pub fn load(&mut self, data:Vec<u8>){
        self.buffer = data;
    }

    pub fn dump_all(&mut self){
        while self.index < self.buffer.len(){
            self.dump();
        }
    }

    fn create_jump_table(&mut self){
        let opcode = self.buffer.get(self.index).unwrap();
        self.index = self.index.wrapping_add(1);
        let mut opcode_arr:[u8;8] = [0;8];
        for n in 0..8 {
            opcode_arr[n] = (opcode & (0b01 << (7-n)))>>(7-n) ;
        }
    }

    pub fn dump(&mut self) {
        print!("{:x} ", self.index);

        let opcode = self.buffer.get(self.index).unwrap();
        self.index = self.index.wrapping_add(1);
        let mut opcode_arr:[u8;8] = [0;8];
        for n in 0..8 {
            opcode_arr[n] = (opcode & (0b01 << (7-n)))>>(7-n) ;
        }
        let rp:usize = ((opcode & 0x30) >> 4) as usize;
        let ddd:usize = ((opcode & 0x38) >> 3) as usize;
        let sss:usize = (opcode & 0x07) as usize;
        let cc = ddd;
        let alu = ddd;
        let n = ddd;

        let bcde:[&str;2] = ["BC","DE"];
        let bdhsp:[&str;4] = ["BD","DE","HL","SP"];
        let bdhpsw:[&str;4] = ["BD","DE","HL","PSW"];
        let bcdehlma:[&str;8] = ["B","C","D","E","H","L","M","A"];
        let aluop1:[&str;8]=["ADD","ADC","SUB","SBB","ANA","XRA","ORA","CMP"];
        let aluop2:[&str;8]=["ADI","ACI","SUI","SBI","ANI","XRI","ORI","CPI"];
        let condition:[&str;8]=["NZ", "Z", "NC", "C", "PO", "PE", "P", "N"];

        // println!("{:.unwrap()}", opcode_arr);
        std::io::stdout().flush().unwrap();


        match opcode_arr {
            [0,0,_,_,0,0,0,0] => println!("NOP"),
            [0,0,_,_,1,0,0,0] => println!("NOP"), // alternative
            [0,0,0,0,0,1,1,1]=>println!("RLC"),
            [0,0,0,0,1,1,1,1]=>println!("RRC"),
            [0,0,0,1,0,1,1,1]=>println!("RAL"),
            [0,0,0,1,1,1,1,1]=>println!("RAR"),
            [0,0,1,0,0,0,1,0]=>{println!("SHLD ${:02x} {:02x}", self.buffer.get(self.index+1).unwrap(), self.buffer.get(self.index).unwrap()); self.index = self.index.wrapping_add(2);},
            [0,0,1,0,0,1,1,1]=>println!("DAA"),
            [0,0,1,0,1,0,1,0]=>{println!("LHLD ${:02x} {:02x}", self.buffer.get(self.index+1).unwrap(), self.buffer.get(self.index).unwrap()); self.index = self.index.wrapping_add(2);},
            [0,0,1,0,1,1,1,1]=>println!("CMA"),
            [0,0,1,1,0,0,1,0]=>{println!("STA ${:02x} {:02x}", self.buffer.get(self.index+1).unwrap(), self.buffer.get(self.index).unwrap()); self.index = self.index.wrapping_add(2);},
            [0,0,1,1,0,1,1,1]=>println!("STC"),
            [0,0,1,1,1,0,1,0]=>{println!("LDA ${:02x} {:02x}", self.buffer.get(self.index+1).unwrap(), self.buffer.get(self.index).unwrap()); self.index = self.index.wrapping_add(2);},
            [0,0,1,1,1,1,1,1]=>println!("CMC"),
            [0,1,1,1,0,1,1,0]=>println!("HLT"),
            [1,1,0,0,0,0,1,1]=>{println!("JMP ${:02x} {:02x}",  self.buffer.get(self.index+1).unwrap(), self.buffer.get(self.index).unwrap()); self.index = self.index.wrapping_add(2);},
            [1,1,0,0,1,0,1,1]=>{println!("JMP ${:02x} {:02x}",  self.buffer.get(self.index+1).unwrap(), self.buffer.get(self.index).unwrap()); self.index = self.index.wrapping_add(2);}, // alternative
            [1,1,0,0,1,0,0,1]=>{println!("RET")},
            [1,1,0,1,1,0,0,1]=>{println!("RET")}, // alternative
            [1,1,0,0,1,1,0,1]=>{println!("CALL ${:02x} {:02x}",  self.buffer.get(self.index+1).unwrap(), self.buffer.get(self.index).unwrap()); self.index = self.index.wrapping_add(2);},
            [1,1,0,1,1,1,0,1]=>{println!("CALL ${:02x} {:02x}",  self.buffer.get(self.index+1).unwrap(), self.buffer.get(self.index).unwrap()); self.index = self.index.wrapping_add(2);}, // alternative
            [1,1,1,0,1,1,0,1]=>{println!("CALL ${:02x} {:02x}",  self.buffer.get(self.index+1).unwrap(), self.buffer.get(self.index).unwrap()); self.index = self.index.wrapping_add(2);}, // alternative
            [1,1,1,1,1,1,0,1]=>{println!("CALL ${:02x} {:02x}",  self.buffer.get(self.index+1).unwrap(), self.buffer.get(self.index).unwrap()); self.index = self.index.wrapping_add(2);},// alternative
            [1,1,0,1,0,0,1,1]=>{println!("OUT {}",self.buffer.get(self.index).unwrap());self.index = self.index.wrapping_add(1)},
            [1,1,0,1,1,0,1,1]=>{println!("IN {}",self.buffer.get(self.index).unwrap());self.index = self.index.wrapping_add(1)},
            [1,1,1,0,0,0,1,1]=>{println!("XTHL")},
            [1,1,1,0,1,0,0,1]=>{println!("PCHL")},
            [1,1,1,0,1,0,1,1]=>{println!("XCHG")},
            [1,1,1,1,0,0,1,1]=>{println!("DI")},
            [1,1,1,1,1,0,0,1]=>{println!("SPHL")},
            [1,1,1,1,1,0,1,1]=>{println!("EI")},
            [0,0,r1,r0,0,0,0,1] => {println!("LXI {} ${:02x} {:02x}", bdhsp[rp], self.buffer.get(self.index+1).unwrap(), self.buffer.get(self.index).unwrap()); self.index = self.index.wrapping_add(2);},
            [0,0,r1,r0,0,0,1,0]=>{println!("STAX {}",bcde[rp])},
            [0,0,r1,r0,0,0,1,1]=>{println!("INX {}", bdhsp[rp])},
            [0,0,d2,d1,d0,1,0,0]=>{println!("INR {}",bcdehlma[ddd])},
            [0,0,d2,d1,d0,1,0,1]=>{println!("DCR {}",bcdehlma[ddd])},
            [0,0,d2,d1,d0,1,1,0]=>{println!("MVI {} {}",bcdehlma[ddd],self.buffer.get(self.index+1).unwrap()); self.index = self.index.wrapping_add(1);},
            [0,0,r1,r0,1,0,0,1]=>{println!("DAD {}", bdhsp[rp])},
            [0,0,r1,r0,1,0,1,0]=>{println!("LDAX {}", bdhsp[rp])},
            [0,0,r1,r0,1,0,1,1]=>{println!("DCX {}", bdhsp[rp])},
            [0,1,d2,d1,d0,s2,s1,s0]=>println!("MOV {},{}",bcdehlma[ddd],bcdehlma[sss]),
            [1,0,alu2,alu1,alu0,s2,s1,s0]=>{println!("ALUOP1 {} {}", aluop1[alu], bcdehlma[sss])},
            [1,1,c2,c1,c0,0,0,0]=>{println!("RCC {}", bcdehlma[sss])},
            [1,1,r1,r0,0,0,0,1]=>{println!("POP {}", bdhpsw[rp])},
            [1,1,c2,c1,c0,0,1,0]=>{println!("JCC {} ${:02x} {:02x}",condition[cc],  self.buffer.get(self.index+1).unwrap(), self.buffer.get(self.index).unwrap()); self.index = self.index.wrapping_add(2);},
            [1,1,c2,c1,c0,1,0,0]=>{println!("CCC {}", condition[cc])},
            [1,1,r1,r0,0,1,0,1]=>{println!("PUSH {}", bdhpsw[rp])},
            [1,1,alu2,alu1,alu0,1,1,0]=>{println!("ALUOP2 {} {}", aluop2[alu], bcdehlma[sss])},
            [1,1,n2,n1,n0,1,1,1]=>{println!("RST {}", n)},
            _ => {
                println!("invalid opcode: {:#b} {:#x} ", opcode, opcode);
            }
        }
    }
}