pub struct ShiftRegister {
    data: u16,
    offset: u8,
}

impl ShiftRegister {
    pub fn new()-> ShiftRegister {
        ShiftRegister { data:0, offset:0 }
    }

    pub fn write_offset(&mut self, offset: u8) {
        self.offset = offset;
    }

    pub fn result(&self)->u8{
        // let mask:u16 = 0xFF00 >> self.offset;
        ((self.data ) >> (8-self.offset)) as u8
    }

    pub fn insert(&mut self, value:u8){
        self.data = self.data >> 8;
        self.data += (value as u16) << 8;
    }
}

mod tests{
    use sdl2::libc::printf;
    use super::*;
    #[test]
    fn test1(){
        let mut sr = ShiftRegister::new();
        sr.insert(0xab);
        assert_eq!(sr.result(), 0xab);
        sr.insert(0xcd);
        assert_eq!(sr.result(), 0xcd);
        sr.write_offset(4);
        assert_eq!(sr.result(), 0xda);
    }

}