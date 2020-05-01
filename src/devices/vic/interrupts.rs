/// TS-7200 VIC Interrupts, as enumerated in section 6.1.2 of the
/// EP93xx User's Guide.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Interrupt {
    Tc1Ui,
    Tc2Ui,
    Uart1RxIntr1,
    Uart1TxIntr1,
    Uart2RxIntr2,
    Uart2TxIntr2,
    Uart3RxIntr3,
    Uart3TxIntr3,
    Tc3Ui,
    IntUart1,
    IntUart2,
    IntUart3,
}

impl Interrupt {
    fn overall_index(self) -> u8 {
        use Interrupt::*;
        match self {
            Tc1Ui => 4,
            Tc2Ui => 5,
            Uart1RxIntr1 => 23,
            Uart1TxIntr1 => 24,
            Uart2RxIntr2 => 25,
            Uart2TxIntr2 => 26,
            Uart3RxIntr3 => 27,
            Uart3TxIntr3 => 28,
            Tc3Ui => 51,
            IntUart1 => 52,
            IntUart2 => 54,
            IntUart3 => 55,
        }
    }

    /// Get VIC bank associated with the interrupt
    pub fn bank(self) -> u8 {
        if self.overall_index() < 32 {
            1
        } else {
            2
        }
    }

    /// Return interrupt index for a specific VIC
    pub fn index(self) -> u8 {
        self.overall_index() & !0x20
    }
}
