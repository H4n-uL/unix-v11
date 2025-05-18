use spin::Mutex;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};

static IDT: Mutex<InterruptDescriptorTable> = Mutex::new(InterruptDescriptorTable::new());

pub fn init_exceptions() {
    let mut idt = IDT.lock();
    idt.breakpoint.set_handler_fn(breakpoint);
    idt.double_fault.set_handler_fn(double_fault);
    unsafe { idt.load_unsafe(); }
}

extern "x86-interrupt" fn breakpoint(_stack: InterruptStackFrame) {
    super::serial_puts("[INTERRUPT] Breakpoint\n");
}

extern "x86-interrupt" fn double_fault(
    _stack: InterruptStackFrame,
    _error_code: u64
) -> ! {
    super::serial_puts("[INTERRUPT] Double Fault\n");
    loop { super::halt(); }
}