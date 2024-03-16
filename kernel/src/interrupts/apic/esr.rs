use bitflags::bitflags;

bitflags! {
    /// The local APIC records errors detected during interrupt handling in the error status
    /// register (ESR).
    ///
    /// The ESR is a write/read register. Before attempt to read from the ESR, software should first
    /// write to it. (The value written does not affect the values read subsequently; only zero may
    /// be written in x2APIC mode.) This write clears any previously logged errors and updates the
    /// ESR with any errors detected since the last write to the ESR. This write also rearms the
    /// APIC error interrupt triggering mechanism.
    ///
    /// The LVT Error Register (see Section 11.5.1) allows specification of the vector of the
    /// interrupt to be delivered to the processor core when APIC error is detected. The register
    /// also provides a means of masking an APIC-error interrupt. This masking only prevents
    /// delivery of APIC-error interrupts; the APIC continues to record errors in the ESR.
    #[derive(Debug)]
    pub struct ErrorStatusRegister: u32 {
        /// Set when the local APIC detects a checksum error for a message that it sent on the APIC
        /// bus. Used only on P6 family and Pentium processors.
        const SEND_CHECKSUM_ERROR = 1 << 0;
        /// Set when the local APIC detects a checksum error for a message that it received on the
        /// APIC bus. Used only on P6 family and Pentium processors.
        const RECEIVE_CHECKSUM_ERROR = 1 << 1;
        /// Set when the local APIC detects that a message it sent was not accepted by any APIC on
        /// the APIC bus. Used only on P6 family and Pentium processors.
        const SEND_ACCEPT_ERROR = 1 << 2;
        /// Set when the local APIC detects that the message it received was not accepted by any
        /// APIC on the APIC bus, including itself. Used only on P6 family and Pentium processors.
        const RECEIVE_ACCEPT_ERROR = 1 << 3;
        /// Set when the local APIC detects an attempt to send an IPI with the lowest-priority
        /// delivery mode and the local APIC does not support the sending of such IPIs. This bit is
        /// used on some Intel Core and Intel Xeon processors. As noted in Section 11.6.2, the
        /// ability of a processor to send a lowest-priority IPI is model-specific and should be
        /// avoided.
        const REDIRECTABLE_IPI = 1 << 4;
        /// Set when the local APIC detects an illegal vector (one in the range 0 to 15) in the
        /// message that it is sending. This occurs as the result of a write to the ICR (in both
        /// xAPIC and x2APIC modes) or to SELF IPI register (x2APIC mode only) with an illegal
        /// vector.
        ///
        /// If the local APIC does not support the sending of lowest-priority IPIs and software
        /// writes the ICR to send a lowest-priority IPI with an illegal vector, the local APIC
        /// sets only the "redirectable IPI" error bit. The interrupt is not processed and hence
        /// the "Send Illegal Vector" bit is not set in the ESR.
        const SEND_ILLEGAL_VECTOR = 1 << 5;
        /// Set when the local APIC detects an illegal vector (one in the range 0 to 15) in an
        /// interrupt message it receives or in an interrupt generated locally from the local
        /// vector table or via a self IPI. Such interrupts are not delivered to the processor;
        /// the local APIC will never set an IRR bit in the range 0 to 15.
        const RECEIVE_ILLEGAL_VECTOR = 1 << 6;
        /// Set when the local APIC is in xAPIC mode and software attempts to access a register
        /// that is reserved in the processor's local-APIC register-address space; see Table 10-1.
        /// (The local-APIC register-address space comprises the 4 KBytes at the physical address
        /// specified in the IA32_APIC_BASE MSR.) Used only on Intel Core, Intel Atom, Pentium 4,
        /// Intel Xeon, and P6 family processors.
        ///
        /// In x2APIC mode, software accesses the APIC registers using the RDMSR and WRMSR
        /// instructions. Use of one of these instructions to access a reserved register cause a
        /// general-protection exception (see Section 10.12.1.3). They do not set the "Illegal
        /// Register Access" bit in the ESR.
        const ILLEGAL_REGISTER_ADDRESS = 1 << 7;
    }
}
