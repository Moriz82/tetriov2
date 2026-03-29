//! Keyboard input — platform-specific implementations.
//! Linux: /dev/uinput virtual keyboard
//! macOS: CGEvent (Core Graphics) key events

use std::thread;
use std::time::Duration;

const KEY_DELAY: Duration = Duration::from_millis(33);

// ─── Platform-specific key codes and input ──────────────────────────

#[cfg(target_os = "linux")]
mod platform {
    use std::fs::{File, OpenOptions};
    use std::io::Write;
    use std::os::unix::io::AsRawFd;
    use std::thread;
    use std::time::Duration;

    const EV_SYN: u16 = 0x00;
    const EV_KEY: u16 = 0x01;
    const SYN_REPORT: u16 = 0x00;

    pub const KEY_Z: u16 = 44;
    pub const KEY_C: u16 = 46;
    pub const KEY_SPACE: u16 = 57;
    pub const KEY_UP: u16 = 103;
    pub const KEY_LEFT: u16 = 105;
    pub const KEY_RIGHT: u16 = 106;

    fn ui_set_evbit() -> libc::c_ulong {
        (1 << 30) | (4 << 16) | ((b'U' as libc::c_ulong) << 8) | 100
    }
    fn ui_set_keybit() -> libc::c_ulong {
        (1 << 30) | (4 << 16) | ((b'U' as libc::c_ulong) << 8) | 101
    }
    fn ui_dev_create() -> libc::c_ulong {
        ((b'U' as libc::c_ulong) << 8) | 1
    }
    fn ui_dev_destroy() -> libc::c_ulong {
        ((b'U' as libc::c_ulong) << 8) | 2
    }

    #[repr(C)]
    struct UinputUserDev {
        name: [u8; 80],
        id_bustype: u16, id_vendor: u16, id_product: u16, id_version: u16,
        ff_effects_max: u32,
        absmax: [i32; 64], absmin: [i32; 64], absfuzz: [i32; 64], absflat: [i32; 64],
    }

    #[repr(C, packed)]
    struct InputEvent {
        tv_sec: i64, tv_usec: i64, type_: u16, code: u16, value: i32,
    }

    pub struct VirtualKeyboard {
        file: File,
    }

    pub fn is_game_focused() -> bool {
        let output = std::process::Command::new("hyprctl")
            .args(["activewindow", "-j"])
            .output();
        match output {
            Ok(o) if o.status.success() => {
                let s = String::from_utf8_lossy(&o.stdout).to_lowercase();
                s.contains("tetr.io") || s.contains("tetrio") || s.contains("tetr io")
            }
            _ => false,
        }
    }

    impl VirtualKeyboard {
        pub fn new() -> Result<Self, String> {
            let file = OpenOptions::new()
                .write(true)
                .open("/dev/uinput")
                .map_err(|e| format!("Cannot open /dev/uinput: {}", e))?;
            let fd = file.as_raw_fd();
            unsafe {
                if libc::ioctl(fd, ui_set_evbit(), EV_KEY as libc::c_int) < 0 {
                    return Err("ioctl UI_SET_EVBIT failed".into());
                }
                for key in [KEY_Z, KEY_C, KEY_SPACE, KEY_UP, KEY_LEFT, KEY_RIGHT] {
                    if libc::ioctl(fd, ui_set_keybit(), key as libc::c_int) < 0 {
                        return Err(format!("ioctl UI_SET_KEYBIT failed for key {}", key));
                    }
                }
                let mut dev = std::mem::MaybeUninit::<UinputUserDev>::zeroed().assume_init();
                let name = b"tetrio-bot-kbd";
                dev.name[..name.len()].copy_from_slice(name);
                dev.id_bustype = 0x03;
                dev.id_vendor = 0x1234;
                dev.id_product = 0x5678;
                dev.id_version = 1;
                let ptr = &dev as *const UinputUserDev as *const libc::c_void;
                let sz = std::mem::size_of::<UinputUserDev>();
                if libc::write(fd, ptr, sz) != sz as isize {
                    return Err("Failed to write uinput_user_dev".into());
                }
                if libc::ioctl(fd, ui_dev_create()) < 0 {
                    return Err("ioctl UI_DEV_CREATE failed".into());
                }
            }
            thread::sleep(Duration::from_millis(300));
            eprintln!("[Input] Virtual keyboard created via /dev/uinput");
            Ok(VirtualKeyboard { file })
        }

        fn write_event(&mut self, type_: u16, code: u16, value: i32) {
            let event = InputEvent { tv_sec: 0, tv_usec: 0, type_, code, value };
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    &event as *const InputEvent as *const u8,
                    std::mem::size_of::<InputEvent>(),
                )
            };
            let _ = self.file.write_all(bytes);
        }

        fn syn(&mut self) { self.write_event(EV_SYN, SYN_REPORT, 0); }

        pub fn press_key(&mut self, code: u16) {
            self.write_event(EV_KEY, code, 1);
            self.syn();
            thread::sleep(Duration::from_millis(5));
            self.write_event(EV_KEY, code, 0);
            self.syn();
        }
    }

    impl Drop for VirtualKeyboard {
        fn drop(&mut self) {
            unsafe { libc::ioctl(self.file.as_raw_fd(), ui_dev_destroy()); }
            eprintln!("[Input] Virtual keyboard destroyed");
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use core_graphics::event::{CGEvent, CGEventTapLocation, CGKeyCode};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    use std::thread;
    use std::time::Duration;

    // macOS virtual key codes
    pub const KEY_Z: u16 = 6;
    pub const KEY_C: u16 = 8;
    pub const KEY_SPACE: u16 = 49;
    pub const KEY_UP: u16 = 126;
    pub const KEY_LEFT: u16 = 123;
    pub const KEY_RIGHT: u16 = 124;

    pub struct VirtualKeyboard {
        source: CGEventSource,
    }

    pub fn is_game_focused() -> bool {
        // On macOS, check if frontmost app contains "tetr" via AppleScript
        let output = std::process::Command::new("osascript")
            .args(["-e", "tell application \"System Events\" to get name of first application process whose frontmost is true"])
            .output();
        match output {
            Ok(o) if o.status.success() => {
                let s = String::from_utf8_lossy(&o.stdout).to_lowercase();
                s.contains("firefox") || s.contains("chrome") || s.contains("safari")
                    || s.contains("librewolf") || s.contains("brave") || s.contains("arc")
            }
            _ => true, // assume focused if we can't check
        }
    }

    impl VirtualKeyboard {
        pub fn new() -> Result<Self, String> {
            let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
                .map_err(|_| "Failed to create CGEventSource".to_string())?;
            eprintln!("[Input] Virtual keyboard created via CGEvent (macOS)");
            Ok(VirtualKeyboard { source })
        }

        pub fn press_key(&mut self, code: u16) {
            let keycode = code as CGKeyCode;
            if let Ok(event) = CGEvent::new_keyboard_event(self.source.clone(), keycode, true) {
                event.post(CGEventTapLocation::HID);
            }
            thread::sleep(Duration::from_millis(5));
            if let Ok(event) = CGEvent::new_keyboard_event(self.source.clone(), keycode, false) {
                event.post(CGEventTapLocation::HID);
            }
        }
    }
}

// ─── Shared interface ───────────────────────────────────────────────

pub use platform::{VirtualKeyboard, is_game_focused};
pub use platform::{KEY_Z, KEY_C, KEY_SPACE, KEY_UP, KEY_LEFT, KEY_RIGHT};

impl VirtualKeyboard {
    /// Returns true if keys were sent, false if game not focused.
    pub fn execute_move(&mut self, use_hold: bool, rotation: u8, dx: i8) -> bool {
        if !is_game_focused() {
            return false;
        }

        if use_hold {
            self.press_key(KEY_C);
            thread::sleep(Duration::from_millis(50));
        }

        match rotation {
            1 => { self.press_key(KEY_UP); thread::sleep(KEY_DELAY); }
            2 => {
                self.press_key(KEY_UP); thread::sleep(KEY_DELAY);
                self.press_key(KEY_UP); thread::sleep(KEY_DELAY);
            }
            3 => { self.press_key(KEY_Z); thread::sleep(KEY_DELAY); }
            _ => {}
        }

        let key = if dx > 0 { KEY_RIGHT } else { KEY_LEFT };
        for _ in 0..dx.unsigned_abs() {
            self.press_key(key);
            thread::sleep(KEY_DELAY);
        }

        thread::sleep(Duration::from_millis(15));
        self.press_key(KEY_SPACE);
        true
    }
}
