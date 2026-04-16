pub struct ConstantStrVec {
    strs: [&'static str; 512],
    len: usize,
}

impl ConstantStrVec {
    pub const fn new() -> Self {
        Self {
            strs: [""; 512],
            len: 0,
        }
    }

    pub const fn get_slice(&self) -> &[&'static str] {
        let slice = unsafe { core::slice::from_raw_parts(self.strs.as_ptr(), self.len) };
        slice
    }

    #[track_caller]
    pub const fn push(&mut self, s: &'static str) {
        if self.len >= 512 {
            panic!("Too many mismatching errors! Cannot show relevant data.");
        }
        self.strs[self.len] = s;
        self.len += 1;
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    // const fn len(&self) -> usize {
    //     self.len
    // }
}
