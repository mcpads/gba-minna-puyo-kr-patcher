use anyhow::{Context, Result, ensure};
use arm7tdmi::{
    Condition, LowRegister, Register, ShiftKind, ThumbAssembler, ThumbImmediateOperation,
    ThumbInstruction as I, ThumbProgram, ThumbTransferWidth,
};

struct Builder<'a> {
    asm: ThumbAssembler,
    origin: u32,
    offset: u32,
    placed: Option<&'a ThumbProgram>,
}
impl<'a> Builder<'a> {
    fn new(origin: u32, placed: Option<&'a ThumbProgram>) -> Self {
        Self {
            asm: ThumbAssembler::new(),
            origin,
            offset: 0,
            placed,
        }
    }
    fn emit(&mut self, instruction: I) {
        self.offset += instruction.encoded_len() as u32;
        self.asm.emit(instruction);
    }
    fn load(&mut self, register: u8, label: &str) -> Result<()> {
        let pc = (self.origin + self.offset + 4) & !3;
        let address = match self.placed {
            Some(p) => p.label_location(label).context("훅 리터럴 누락")?,
            None => pc,
        };
        let offset = address.checked_sub(pc).context("훅 리터럴 역방향")?;
        self.emit(I::PcRelativeLoad {
            destination: LowRegister::new(register)?,
            offset: offset.try_into()?,
        });
        Ok(())
    }
    fn immediate(
        &mut self,
        register: u8,
        value: u8,
        operation: ThumbImmediateOperation,
    ) -> Result<()> {
        self.emit(I::Immediate {
            operation,
            register: LowRegister::new(register)?,
            immediate: value,
        });
        Ok(())
    }
    fn pool(&mut self, words: &[(&str, u32)]) {
        let pad = (4 - (self.origin + self.offset) % 4) % 4;
        self.asm.data(vec![0; pad as usize]);
        self.offset += pad;
        for &(name, value) in words {
            self.asm.label(name).data(value.to_le_bytes().to_vec());
            self.offset += 4;
        }
    }
}
fn selector_pass(
    origin: u32,
    korean: u32,
    map: bool,
    placed: Option<&ThumbProgram>,
) -> Result<ThumbProgram> {
    let mut b = Builder::new(origin, placed);
    b.load(0, "language")?;
    b.emit(I::ImmediateTransfer {
        load: true,
        width: ThumbTransferWidth::Halfword,
        destination: LowRegister::new(0)?,
        base: LowRegister::new(0)?,
        offset: 0,
    });
    b.immediate(0, 0, ThumbImmediateOperation::Compare)?;
    b.asm.branch_to_label(Condition::NotEqual, "english");
    b.offset += 2;
    b.load(0, "korean")?;
    b.asm.jump_label("selected");
    b.offset += 2;
    b.asm.label("english");
    b.load(0, "original")?;
    b.asm.label("selected");
    if map {
        b.load(1, "destination")?;
        b.immediate(2, 20, ThumbImmediateOperation::Move)?;
        b.emit(I::SpRelativeTransfer {
            load: false,
            register: LowRegister::new(2)?,
            offset: 0,
        });
        b.immediate(2, 0, ThumbImmediateOperation::Move)?;
    } else {
        b.immediate(1, 0xc0, ThumbImmediateOperation::Move)?;
        b.emit(I::ShiftImmediate {
            kind: ShiftKind::LogicalLeft,
            destination: LowRegister::new(1)?,
            source: LowRegister::new(1)?,
            amount: 19,
        });
        b.immediate(2, 0xc0, ThumbImmediateOperation::Move)?;
    }
    b.load(3, "return")?;
    b.emit(I::BranchExchange {
        source: Register::new(3)?,
    });
    b.pool(&[
        ("language", 0x03000356),
        ("korean", korean),
        ("original", if map { 0x080ea190 } else { 0x080e7190 }),
        ("return", if map { 0x0801f335 } else { 0x0801f325 }),
        ("destination", if map { 0x0600f000 } else { 0x06000000 }),
    ]);
    Ok(b.asm.assemble(origin)?)
}
pub fn selector(origin: u32, korean: u32, map: bool) -> Result<ThumbProgram> {
    let layout = selector_pass(origin, korean, map, None)?;
    let result = selector_pass(origin, korean, map, Some(&layout))?;
    ensure!(result.bytes().len() == layout.bytes().len(), "훅 배치 변경");
    for name in [
        "language",
        "korean",
        "original",
        "return",
        "destination",
        "english",
        "selected",
    ] {
        ensure!(
            result.label_location(name) == layout.label_location(name),
            "훅 레이블 배치 변경"
        );
    }
    Ok(result)
}
pub fn entry(origin: u32, destination: u32) -> Result<ThumbProgram> {
    let mut first = Builder::new(origin, None);
    first.load(0, "target")?;
    first.emit(I::BranchExchange {
        source: Register::new(0)?,
    });
    first.pool(&[("target", destination | 1)]);
    let layout = first.asm.assemble(origin)?;
    let mut final_pass = Builder::new(origin, Some(&layout));
    final_pass.load(0, "target")?;
    final_pass.emit(I::BranchExchange {
        source: Register::new(0)?,
    });
    final_pass.pool(&[("target", destination | 1)]);
    Ok(final_pass.asm.assemble(origin)?)
}
pub fn report(program: &ThumbProgram) -> serde_json::Value {
    serde_json::json!({"origin":program.origin(),"bytes":program.bytes().len(),"sha256":crate::source::sha256(program.bytes()),
        "instructions":program.instruction_spans().iter().map(|s|serde_json::json!({"address":s.location,"offset":s.offset,"instruction":format!("{:?}",s.instruction)})).collect::<Vec<_>>()})
}
