pub struct SimpleAlloc {
    mapping: Vec<Option<usize>>,
    frame_size: usize,
}

impl SimpleAlloc {
    pub fn new(reg_count: usize) -> Self {
        Self {
            mapping: vec![None; reg_count],
            frame_size: reg_count * 8,
        }
    }

    pub fn map(&mut self, virt: usize) -> usize {
        if let Some(phys) = self.mapping[virt] {
            return phys;
        }
        let phys = virt % PHYS_REGS.len();
        self.mapping[virt] = Some(phys);
        phys
    }

    pub fn frame_size(&self) -> usize {
        self.frame_size
    }

    pub fn is_mapped(&self, phys: usize) -> bool {
        self.mapping.iter().any(|m| *m == Some(phys))
    }
}

pub const PHYS_REGS: &[&str] = &[
    "rax", "rcx", "rdx", "rbx", "rsi", "rdi", "r8", "r9",
    "r10", "r11", "r12", "r13", "r14", "r15", "rbp",
];

pub fn virt_to_name(virt: usize) -> &'static str {
    PHYS_REGS[virt % PHYS_REGS.len()]
}
