pub mod apic;

use x86_64::{
    PhysAddr,
    instructions::port::Port,
    registers::model_specific::Msr,
    structures::idt::{InterruptDescriptorTable, InterruptStackFrame},
};

use apic::ApicRegs;

enum Interrupts {
    ApicTimer = 48,
    ApicError,
    ApicSpurious,
}

pub static IDT: spin::Lazy<InterruptDescriptorTable> = spin::Lazy::new(|| {
    let mut idt = InterruptDescriptorTable::new();
    idt.breakpoint.set_handler_fn(breakpoint_handler);
    let double_fault_options = idt.double_fault.set_handler_fn(double_fault_handler);
    unsafe { double_fault_options.set_stack_index(crate::gdt::DOUBLE_FAULT_IST_INDEX) };
    idt[Interrupts::ApicTimer as u8].set_handler_fn(apic_timer_handler);
    idt[Interrupts::ApicError as u8].set_handler_fn(apic_error_handler);
    idt[Interrupts::ApicSpurious as u8].set_handler_fn(apic_spurious_handler);
    idt
});

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    log::info!("BREAKPOINT\n{stack_frame:#?}");

    crate::sprintln!("Hello there");
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    panic!("DOUBLE FAULT:\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn apic_timer_handler(_stack_frame: InterruptStackFrame) {
    let mut regs = APIC_REGS.get().unwrap().clone();
    log::info!("TIMER INTERRUPT");
    unsafe { regs.end_interrupt(()) };
}

extern "x86-interrupt" fn apic_error_handler(_stack_frame: InterruptStackFrame) {
    let mut regs = APIC_REGS.get().unwrap().clone();
    log::info!("ERROR: {:?}", unsafe { regs.read_error_status() });
    unsafe { regs.end_interrupt(()) };
}

extern "x86-interrupt" fn apic_spurious_handler(stack_frame: InterruptStackFrame) {
    let mut regs = APIC_REGS.get().unwrap().clone();
    log::info!("ERROR: {stack_frame:#?}");
    unsafe { regs.end_interrupt(()) };
}

pub fn init_idt() {
    IDT.load();
}

unsafe fn wait() {
    unsafe { Port::new(0x80).write(0u8) };
}

// disable the 8259 PIC
unsafe fn disable_pic8259() {
    let mut pic1_command = Port::<u8>::new(0x20);
    let mut pic2_command = Port::<u8>::new(0xA0);
    let mut pic1_data = Port::<u8>::new(0x21);
    let mut pic2_data = Port::<u8>::new(0xA1);

    let mut write_data = |data1, data2| unsafe {
        pic1_data.write(data1);
        wait();
        pic2_data.write(data2);
        wait();
    };

    unsafe {
        // Tell each PIC that we're going to send it a three-byte
        // initialization sequence on its data port.
        pic1_command.write(0x11);
        wait();
        pic2_command.write(0x11);
        wait();

        // Set the base interrupt number for each PIC
        write_data(32, 40);
        // Configure chaining between the PICs
        write_data(4, 2);
        // Set the mode of each PIC
        write_data(1, 1);

        // Disable the PICS by masking all interrupts
        write_data(0xFF, 0xFF);
    }
}

const IA_APIC_BASE_MSR: u32 = 0x1B;
/// Indicates if the processor is the bootstrap processor (BSP). See Section 9.4, "Multiple-Processor (MP)
/// Initialization." Following a power-up or reset, this flag is set to 1 for the processor selected as
/// the BSP and set to 0 for the remaining processors (APs).
const _IA_APIC_BASE_MSR_BSP: u64 = 1 << 8;
const IA_APIC_BASE_MSR_ENABLE: u64 = 1 << 11;
const IA_APIC_BASE_MSR_X2APIC: u64 = 1 << 10;

static APIC_REGS: spin::Once<ApicRegs> = spin::Once::new();

pub unsafe fn init_apic() {
    unsafe { disable_pic8259() };
    let Some(feature_info) = raw_cpuid::CpuId::new().get_feature_info() else {
        panic!("Feature information not available");
    };

    if !feature_info.has_apic() {
        panic!("APIC not available");
    }

    let x2apic = feature_info.has_x2apic();

    let mut apic_base_msr = Msr::new(IA_APIC_BASE_MSR);
    let mut apic_base_value = unsafe { apic_base_msr.read() } | IA_APIC_BASE_MSR_ENABLE;
    if x2apic {
        apic_base_value |= IA_APIC_BASE_MSR_X2APIC;
    }
    unsafe {
        apic_base_msr.write(apic_base_value);
    }

    // should be 0xFEE0_0000
    let apic_base_addr = PhysAddr::new_truncate(apic_base_value & !4095);
    let Some(apic_base_addr) = (unsafe {
        crate::memory::VMM
            .get()
            .expect("VMM not initialized")
            .lock()
            .map(true, 4096, 12, apic_base_addr)
    }) else {
        panic!("Virtual memory mapping failed");
    };

    let mut regs = APIC_REGS
        .call_once(|| unsafe { ApicRegs::new(x2apic, apic_base_addr.as_mut_ptr()) })
        .clone();

    unsafe {
        let mut lvt = regs.read_lvt_timer();
        lvt.set_mask(false);
        lvt.set_vector(Interrupts::ApicTimer as _);
        lvt.set_timer_mode(apic::lvt::TimerMode::Periodic);
        regs.write_lvt_timer(lvt);

        regs.write_timer_div(apic::DivideConfigurationRegister::DivideBy128);
        regs.write_timer_init(1 << 20);

        let mut lvt = regs.read_lvt_error();
        lvt.set_mask(false);
        lvt.set_vector(Interrupts::ApicError as _);
        regs.write_lvt_error(lvt);

        let mut svr = regs.read_svr();
        svr.set_vector(Interrupts::ApicSpurious as _);
        regs.write_svr(svr);
    }
    drop(regs);

    x86_64::instructions::interrupts::enable();
}
