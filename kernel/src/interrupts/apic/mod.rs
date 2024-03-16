pub mod esr;
pub mod icr;
pub mod lapic_ver;
pub mod lvt;
pub mod prio_reg;
pub mod svr;

use x86_64::{instructions::interrupts::without_interrupts, registers::model_specific::Msr};

use esr::ErrorStatusRegister;
use icr::InterruptCommandRegister;
use lapic_ver::LocalAPICVersion;
use lvt::LocalVectorTable;
use svr::SpuriousInterruptVectorRegister;

use self::prio_reg::PriorityRegisiter;

/// Selects the trigger mode for the local LINT0 and LINT1 pins: edge sensitive and level sensitive.
/// This flag is only used when the delivery mode is Fixed. When the delivery mode is NMI, SMI, or
/// INIT, the trigger mode is always edge sensitive. When the delivery mode is ExtINT, the trigger
/// mode is always level sensitive. The timer and error interrupts are always treated as edge
/// sensitive.
///
/// If the local APIC is not used in conjunction with an I/O APIC and fixed delivery mode is
/// selected; the Pentium 4, Intel Xeon, and P6 family processors will always use level-sensitive
/// triggering, regardless if edge-sensitive triggering is selected.
///
/// Software should always set the trigger mode in the LVT LINT1 register to `Edge`.
/// Level-sensitive interrupts are not supported for LINT1
#[derive(Debug)]
pub enum TriggerMode {
    /// Edge sensitive
    Edge = 0,
    /// Level sensitive
    Level = 1,
}

trait X2ApicReadReg {
    fn read_reg32(reg32: u32) -> Self;
}

trait X2ApicWriteReg {
    fn write_reg32(value: Self) -> u32;
}

macro_rules! impl_basic_x2apic_reg32 {
    ($ty:ty) => {
        impl X2ApicReadReg for $ty {
            fn read_reg32(reg32: u32) -> Self {
                Self::from_bits_retain(reg32)
            }
        }
        impl X2ApicWriteReg for $ty {
            fn write_reg32(value: Self) -> u32 {
                value.bits() as _
            }
        }
    };
}

impl_basic_x2apic_reg32!(LocalVectorTable);
impl_basic_x2apic_reg32!(SpuriousInterruptVectorRegister);
impl_basic_x2apic_reg32!(ErrorStatusRegister);
impl_basic_x2apic_reg32!(LocalAPICVersion);
impl_basic_x2apic_reg32!(PriorityRegisiter);

impl X2ApicReadReg for u32 {
    fn read_reg32(msr: u32) -> Self {
        msr
    }
}
impl X2ApicWriteReg for u32 {
    fn write_reg32(value: Self) -> u32 {
        value
    }
}

impl X2ApicWriteReg for () {
    fn write_reg32(_: Self) -> u32 {
        0
    }
}

/// Divide Configuration Register (DCR; for Timer)
pub enum DivideConfigurationRegister {
    DivideBy1 = 0b1011,
    DivideBy2 = 0b0000,
    DivideBy4 = 0b0001,
    DivideBy8 = 0b0010,
    DivideBy16 = 0b0011,
    DivideBy32 = 0b1000,
    DivideBy64 = 0b1001,
    DivideBy128 = 0b1010,
}

impl X2ApicReadReg for DivideConfigurationRegister {
    fn read_reg32(msr: u32) -> Self {
        match msr {
            0b1011 => Self::DivideBy1,
            0b0000 => Self::DivideBy2,
            0b0001 => Self::DivideBy4,
            0b0010 => Self::DivideBy8,
            0b0011 => Self::DivideBy16,
            0b1000 => Self::DivideBy32,
            0b1001 => Self::DivideBy64,
            0b1010 => Self::DivideBy128,
            _ => unimplemented!("reserved"),
        }
    }
}
impl X2ApicWriteReg for DivideConfigurationRegister {
    fn write_reg32(value: Self) -> u32 {
        value as _
    }
}

#[derive(Clone)]
pub struct ApicRegs {
    x2apic: bool,
    base_addr: *mut u32,
}

unsafe impl Sync for ApicRegs {}
unsafe impl Send for ApicRegs {}

impl ApicRegs {
    pub unsafe fn new(x2apic: bool, base_addr: *mut u32) -> Self {
        Self { x2apic, base_addr }
    }
}

macro_rules! apic_regs {
    () => {};
    (
        $(#[$attr:meta])*
        $name:ident: Read<$ty:ty, $msr_addr:literal, $offset_addr:literal $(,)?> $(,)?
    ) => {
        $(#[$attr])*
        pub unsafe fn $name(&self) -> $ty {
            let val = if self.x2apic {
                let msr = Msr::new($msr_addr);
                unsafe { msr.read() as _ }
            } else {
                unsafe { self.base_addr.byte_add($offset_addr).read_volatile() as _ }
            };
            X2ApicReadReg::read_reg32(val)
        }
    };
    (
        $(#[$attr:meta])*
        $name:ident: Write<$ty:ty, $msr_addr:literal, $offset_addr:literal $(,)?>$(,)?
    ) => {
        $(#[$attr])*
        pub unsafe fn $name(&mut self, value: $ty) {
            let val = X2ApicWriteReg::write_reg32(value);
            if self.x2apic {
                let mut msr = Msr::new($msr_addr);
                unsafe { msr.write(val as _) };
            } else {
                unsafe { self.base_addr.byte_add($offset_addr).write_volatile(val as _) };
            }
        }
    };
    (
        $(
            $(#[$attr:meta])*
            $name:ident: $act:ident<$ty:ty, $msr_addr:literal, $offset_addr:literal $(,)?>
        ),+ $(,)?
    ) => {
        $(
            apic_regs! {
                $(#[$attr])*
                $name: $act<$ty, $msr_addr, $offset_addr>
            }
        )*
    };
}

impl ApicRegs {
    apic_regs! {
        /// Local APIC ID Register, See Section 11.12.5.1 for initial values.
        read_lapid_id: Read<u32, 0x802, 0x020>,
        /// Local APIC Version Register, Same version used in xAPIC mode and x2APIC mode.
        read_lapic_ver: Read<LocalAPICVersion, 0x803, 0x030>,
        /// Task Priority Register (TPR), Bits 31:8 are reserved and must be written with zeros.
        read_tpr: Read<PriorityRegisiter, 0x803, 0x080>,
        /// Task Priority Register (TPR), Bits 31:8 are reserved and must be written with zeros.
        write_tpr: Write<PriorityRegisiter, 0x803, 0x080>,
        /// Processor Priority Register (PPR)
        read_ppr: Read<PriorityRegisiter, 0x80A, 0x0A0>,
        /// End of interrupt register
        end_interrupt: Write<(), 0x80B, 0x0B0>,
        /// Logical Destination Register (LDR)
        read_ldr: Read<u32, 0x80D, 0x0D0>,
    }

    /// Destination Format Register (DFR)
    ///
    /// Only available in xAPIC (not x2APIC).
    pub unsafe fn read_dfr(&mut self) -> u32 {
        unsafe { self.base_addr.byte_add(0x0E0).read_volatile() }
    }

    /// Destination Format Register (DFR)
    ///
    /// Only available in xAPIC (not x2APIC).
    pub unsafe fn write_dfr(&mut self, value: u32) {
        unsafe { self.base_addr.byte_add(0x0E0).write_volatile(value) };
    }

    apic_regs! {
        /// Spurious Interrupt Vector Register (SVR), See Section 11.9 for reserved bits.
        read_svr: Read<SpuriousInterruptVectorRegister, 0x80F, 0x0F0>,
        /// Spurious Interrupt Vector Register (SVR), See Section 11.9 for reserved bits.
        write_svr: Write<SpuriousInterruptVectorRegister, 0x80F, 0x0F0>,

        /// In-Service Register (ISR); bits 31:0
        read_isr0: Read<u32, 0x810, 0x100>,
        /// In-Service Register (ISR); bits 63:32
        read_isr1: Read<u32, 0x811, 0x110>,
        /// In-Service Register (ISR); bits 95:64
        read_isr2: Read<u32, 0x812, 0x120>,
        /// In-Service Register (ISR); bits 127:96
        read_isr3: Read<u32, 0x813, 0x130>,
        /// In-Service Register (ISR); bits 159:128
        read_isr4: Read<u32, 0x814, 0x140>,
        /// In-Service Register (ISR); bits 191:160
        read_isr5: Read<u32, 0x815, 0x150>,
        /// In-Service Register (ISR); bits 223:192
        read_isr6: Read<u32, 0x816, 0x160>,
        /// In-Service Register (ISR); bits 255:224
        read_isr7: Read<u32, 0x817, 0x170>,

        /// Trigger Mode Register (TMR); bits 31:0
        read_tmr0: Read<u32, 0x818, 0x180>,
        /// Trigger Mode Register (TMR); bits 63:32
        read_tmr1: Read<u32, 0x819, 0x190>,
        /// Trigger Mode Register (TMR); bits 95:64
        read_tmr2: Read<u32, 0x81A, 0x1A0>,
        /// Trigger Mode Register (TMR); bits 127:96
        read_tmr3: Read<u32, 0x81B, 0x1B0>,
        /// Trigger Mode Register (TMR); bits 159:128
        read_tmr4: Read<u32, 0x81C, 0x1C0>,
        /// Trigger Mode Register (TMR); bits 191:160
        read_tmr5: Read<u32, 0x81D, 0x1D0>,
        /// Trigger Mode Register (TMR); bits 223:192
        read_tmr6: Read<u32, 0x81E, 0x1E0>,
        /// Trigger Mode Register (TMR); bits 255:224
        read_tmr7: Read<u32, 0x81F, 0x1F0>,

        /// Interrupt Request Register (IRR); bits 31:0
        read_irr0: Read<u32, 0x820, 0x200>,
        /// Interrupt Request Register (IRR); bits 63:32
        read_irr1: Read<u32, 0x821, 0x210>,
        /// Interrupt Request Register (IRR); bits 95:64
        read_irr2: Read<u32, 0x822, 0x220>,
        /// Interrupt Request Register (IRR); bits 127:96
        read_irr3: Read<u32, 0x823, 0x230>,
        /// Interrupt Request Register (IRR); bits 159:128
        read_irr4: Read<u32, 0x824, 0x240>,
        /// Interrupt Request Register (IRR); bits 191:160
        read_irr5: Read<u32, 0x825, 0x250>,
        /// Interrupt Request Register (IRR); bits 223:192
        read_irr6: Read<u32, 0x826, 0x260>,
        /// Interrupt Request Register (IRR); bits 255:224
        read_irr7: Read<u32, 0x827, 0x270>,

        /// Error Status Register (ESR), WRMSR of a non-zero value causes #GP(0). See Section 11.5.3.
        read_error_status: Read<ErrorStatusRegister, 0x828, 0x280>,
        /// Error Status Register (ESR)
        write_error_status: Write<(), 0x828, 0x280>,
        /// Specifies interrupt delivery when an overflow condition of corrected machine check error count
        /// reaching a threshold value occurred in a machine check bank supporting CMCI (see Section 16.5.1,
        /// "CMCI Local APIC Interface").
        read_lvt_cmci: Read<LocalVectorTable, 0x82F, 0x2F0>,
        /// Specifies interrupt delivery when an overflow condition of corrected machine check error count
        /// reaching a threshold value occurred in a machine check bank supporting CMCI (see Section 16.5.1,
        /// "CMCI Local APIC Interface").
        write_lvt_cmci: Write<LocalVectorTable, 0x82F, 0x2F0>,
    }

    /// Interrupt Command Register (ICR)
    pub unsafe fn read_icr(&self) -> InterruptCommandRegister {
        if self.x2apic {
            let msr = Msr::new(0x830);
            let value = unsafe { msr.read() };
            InterruptCommandRegister(value)
        } else {
            without_interrupts(|| {
                let higher = unsafe { self.base_addr.byte_add(0x310).read_volatile() } as u64;
                let lower = unsafe { self.base_addr.byte_add(0x300).read_volatile() } as u64;
                InterruptCommandRegister(higher << 32 | lower)
            })
        }
    }
    /// Interrupt Command Register (ICR)
    pub unsafe fn write_icr(&mut self, value: InterruptCommandRegister) {
        if self.x2apic {
            let mut msr = Msr::new(0x830);
            unsafe { msr.write(value.0) };
        } else {
            without_interrupts(|| {
                let addr = self.base_addr;
                unsafe { addr.byte_add(0x310).write_volatile((value.0 >> 32) as _) };
                unsafe { addr.byte_add(0x300).write_volatile(value.0 as _) };
            })
        }
    }

    apic_regs! {
        /// Specifies interrupt delivery when the APIC timer signals an interrupt (see Section 11.5.4, "APIC Timer").
        read_lvt_timer: Read<LocalVectorTable, 0x832, 0x320>,
        /// Specifies interrupt delivery when the APIC timer signals an interrupt (see Section 11.5.4, "APIC Timer").
        write_lvt_timer: Write<LocalVectorTable, 0x832, 0x320>,
        /// Specifies interrupt delivery when the thermal sensor generates an interrupt (see Section 15.8.2,
        /// "Thermal Monitor"). This LVT entry is implementation specific, not architectural.
        read_lvt_thermal: Read<LocalVectorTable, 0x833, 0x330>,
        /// Specifies interrupt delivery when the thermal sensor generates an interrupt (see Section 15.8.2,
        /// "Thermal Monitor"). This LVT entry is implementation specific, not architectural.
        write_lvt_thermal: Write<LocalVectorTable, 0x833, 0x330>,
        /// Specifies interrupt delivery when a performance counter generates an interrupt on overflow
        /// (see Section 20.6.3.5.8, "Generating an Interrupt on Overflow") or when Intel PT signals a ToPA PMI
        /// (see Section 33.2.7.2). This LVT entry is implementation specific, not architectural.
        read_lvt_perfmon: Read<LocalVectorTable, 0x834, 0x340>,
        /// Specifies interrupt delivery when a performance counter generates an interrupt on overflow
        /// (see Section 20.6.3.5.8, "Generating an Interrupt on Overflow") or when Intel PT signals a ToPA PMI
        /// (see Section 33.2.7.2). This LVT entry is implementation specific, not architectural.
        write_lvt_perfmon: Write<LocalVectorTable, 0x834, 0x340>,
        /// Specifies interrupt delivery when an interrupt is signaled at the LINT0 pin.
        read_lvt_lint0: Read<LocalVectorTable, 0x835, 0x350>,
        /// Specifies interrupt delivery when an interrupt is signaled at the LINT0 pin.
        write_lvt_lint0: Write<LocalVectorTable, 0x835, 0x350>,
        /// Specifies interrupt delivery when an interrupt is signaled at the LINT1 pin.
        read_lvt_lint1: Read<LocalVectorTable, 0x836, 0x360>,
        /// Specifies interrupt delivery when an interrupt is signaled at the LINT1 pin.
        write_lvt_lint1: Write<LocalVectorTable, 0x836, 0x360>,
        /// Specifies interrupt delivery when the APIC detects an internal error (see Section 11.5.3, "Error Handling").
        read_lvt_error: Read<LocalVectorTable, 0x837, 0x370>,
        /// Specifies interrupt delivery when the APIC detects an internal error (see Section 11.5.3, "Error Handling").
        write_lvt_error: Write<LocalVectorTable, 0x837, 0x370>,
        /// Initial Count register (for Timer)
        read_initial_count: Read<u32, 0x838, 0x380>,
        /// Initial Count register (for Timer)
        write_timer_init: Write<u32, 0x838, 0x380>,
        /// Current Count register (for Timer)
        read_current_count: Read<u32, 0x839, 0x390>,
        /// Divide Configuration Register (DCR; for Timer).
        read_dcr: Read<DivideConfigurationRegister, 0x83E, 0x3E0>,
        /// Divide Configuration Register (DCR; for Timer)
        write_timer_div: Write<DivideConfigurationRegister, 0x83E, 0x3E0>,
    }

    /// TODO: what is SELF IPI
    ///
    /// Only available in x2APIC (not xAPIC).
    pub unsafe fn write_self_ipi(&mut self, value: u32) {
        let mut msr = Msr::new(0x83F);
        unsafe { msr.write(value as _) };
    }
}
