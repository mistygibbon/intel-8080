# Intel 8080 Emulator

![A screenshot of the emulator](./screenshot.png)

An Intel 8080 emulator written in Rust, running space invaders
- Passes 8080EXM, 8080PRE and TST8080 tests.
- Supports sounds (Create folder named samples/, put "0.wav-9.wav" inside)

## Usage
Please install SDL2 with headers as well as SDL2 mixer with headers, then run
```bash
cargo run
```

## Controls
- Insert Coin: C
- 1 Player Start: G
- 2 Player Start: T
### Player 1
- Left: A
- Right: D
- Fire: F
### Player 2
- Left: Left Arrow
- Right: Right Arrow
- Fire: Slash

## References
- [Opcode table](https://pastraiser.com/cpu/i8080/i8080_opcodes.html)
- [CPU Test ROMs](https://github.com/superzazu/8080/tree/master/cpu_tests)
- [emulator101.com](https://web.archive.org/web/20180727123034/http://www.emulator101.com/)
- [Computer Archaeology](https://web.archive.org/web/20180718140153/http://computerarcheology.com/Arcade/SpaceInvaders/)