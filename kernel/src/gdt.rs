use x86_64::{
    VirtAddr,
    structures::{
        gdt::{self, GlobalDescriptorTable},
        tss::TaskStateSegment,
    },
};

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

static TSS: spin::Lazy<TaskStateSegment> = spin::Lazy::new(|| {
    let mut tss = TaskStateSegment::new();
    tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
        const STACK_SIZE: usize = 20 << 10;
        static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];
        let stack_start = VirtAddr::from_ptr(&raw mut STACK);
        let stack_end = stack_start + STACK_SIZE as u64;
        stack_end
    };
    tss
});

struct Gdt {
    gdt: GlobalDescriptorTable,
    code_selector: gdt::SegmentSelector,
    data_selector: gdt::SegmentSelector,
    tss_selector: gdt::SegmentSelector,
}

static GDT: spin::Lazy<Gdt> = spin::Lazy::new(|| {
    let mut gdt = GlobalDescriptorTable::new();
    let code_selector = gdt.append(gdt::Descriptor::kernel_code_segment());
    let data_selector = gdt.append(gdt::Descriptor::kernel_data_segment());
    let tss_selector = gdt.append(gdt::Descriptor::tss_segment(&TSS));
    Gdt {
        gdt,
        code_selector,
        data_selector,
        tss_selector,
    }
});

pub fn init() {
    use x86_64::{
        instructions::segmentation::{CS, DS, SS, Segment},
        instructions::tables::load_tss,
    };
    GDT.gdt.load();
    unsafe {
        CS::set_reg(GDT.code_selector);
        SS::set_reg(GDT.data_selector);
        DS::set_reg(GDT.data_selector);
        load_tss(GDT.tss_selector);
    }
}
