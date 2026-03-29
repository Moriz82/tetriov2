//! Keyboard input via Linux uinput — works on any Wayland compositor.
//! Creates a virtual keyboard device and sends key events directly through the kernel.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::thread;
use std::time::Duration;

const KEY_DELAY: Duration = Duration::from_millis(33);

// Linux input event constants
const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const SYN_REPORT: u16 = 0x00;

// Key codes (linux/input-event-codes.h)
const KEY_Z: u16 = 44;
const KEY_A: u16 = 30;
const KEY_C: u16 = 46;
const KEY_SPACE: u16 = 57;
const KEY_UP: u16 = 103;
const KEY_LEFT: u16 = 105;
const KEY_RIGHT: u16 = 106;

// uinput ioctl numbers
fn ui_set_evbit() -> libc::c_ulong {
    // _IOW('U', 100, int)  = direction(1)<<30 | size(4)<<16 | type('U')<<8 | nr(100)
    (1 << 30) | (4 << 16) | ((b'U' as libc::c_ulong) << 8) | 100
}
fn ui_set_keybit() -> libc::c_ulong {
    (1 << 30) | (4 << 16) | ((b'U' as libc::c_ulong) << 8) | 101
}
fn ui_dev_create() -> libc::c_ulong {
    // _IO('U', 1)  = type<<8 | nr
    ((b'U' as libc::c_ulong) << 8) | 1
}
fn ui_dev_destroy() -> libc::c_ulong {
    ((b'U' as libc::c_ulong) << 8) | 2
}

#[repr(C)]
struct UinputUserDev {
    name: [u8; 80],
    id_bustype: u16,
    id_vendor: u16,
    id_product: u16,
    id_version: u16,
    ff_effects_max: u32,
    absmax: [i32; 64],
    absmin: [i32; 64],
    absfuzz: [i32; 64],
    absflat: [i32; 64],
}

#[repr(C, packed)]
struct InputEvent {
    tv_sec: i64,
    tv_usec: i64,
    type_: u16,
    code: u16,
    value: i32,
}

pub struct VirtualKeyboard {
    file: File,
    enabled: bool,
}

/// Check if the focused window is likely TETR.IO (browser with game).
pub fn is_game_focused() -> bool {
    let output = std::process::Command::new("hyprctl")
        .args(["activewindow", "-j"])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout).to_lowercase();
            // Match common browser window titles/classes
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
            // Enable EV_KEY
            if libc::ioctl(fd, ui_set_evbit(), EV_KEY as libc::c_int) < 0 {
                return Err("ioctl UI_SET_EVBIT failed".into());
            }

            // Enable the keys we need
            for key in [KEY_Z, KEY_A, KEY_C, KEY_SPACE, KEY_UP, KEY_LEFT, KEY_RIGHT] {
                if libc::ioctl(fd, ui_set_keybit(), key as libc::c_int) < 0 {
                    return Err(format!("ioctl UI_SET_KEYBIT failed for key {}", key));
                }
            }

            // Setup device using the legacy uinput_user_dev struct
            let mut dev = std::mem::MaybeUninit::<UinputUserDev>::zeroed().assume_init();
            let name = b"tetrio-bot-kbd";
            dev.name[..name.len()].copy_from_slice(name);
            dev.id_bustype = 0x03; // BUS_USB
            dev.id_vendor = 0x1234;
            dev.id_product = 0x5678;
            dev.id_version = 1;

            let ptr = &dev as *const UinputUserDev as *const libc::c_void;
            let sz = std::mem::size_of::<UinputUserDev>();
            if libc::write(fd, ptr, sz) != sz as isize {
                return Err("Failed to write uinput_user_dev".into());
            }

            // Create the device
            if libc::ioctl(fd, ui_dev_create()) < 0 {
                return Err("ioctl UI_DEV_CREATE failed".into());
            }
        }

        // Give the system time to register the device
        thread::sleep(Duration::from_millis(300));

        eprintln!("[Input] Virtual keyboard created via /dev/uinput");
        Ok(VirtualKeyboard { file, enabled: true })
    }

    fn write_event(&mut self, type_: u16, code: u16, value: i32) {
        let event = InputEvent {
            tv_sec: 0,
            tv_usec: 0,
            type_,
            code,
            value,
        };
        let bytes = unsafe {
            std::slice::from_raw_parts(
                &event as *const InputEvent as *const u8,
                std::mem::size_of::<InputEvent>(),
            )
        };
        let _ = self.file.write_all(bytes);
    }

    fn syn(&mut self) {
        self.write_event(EV_SYN, SYN_REPORT, 0);
    }

    pub fn press_key(&mut self, code: u16) {
        self.write_event(EV_KEY, code, 1); // key down
        self.syn();
        thread::sleep(Duration::from_millis(5));
        self.write_event(EV_KEY, code, 0); // key up
        self.syn();
    }

    /// Returns true if keys were sent, false if game not focused.
    pub fn execute_move(&mut self, use_hold: bool, rotation: u8, dx: i8) -> bool {
        // Safety: only send keys if game window is focused
        if !is_game_focused() {
            return false;
        }

        if use_hold {
            self.press_key(KEY_C);
            thread::sleep(Duration::from_millis(50)); // wait for hold swap
        }

        match rotation {
            1 => { self.press_key(KEY_UP); thread::sleep(KEY_DELAY); }  // CW
            2 => {
                // 180 = two CW rotations
                self.press_key(KEY_UP); thread::sleep(KEY_DELAY);
                self.press_key(KEY_UP); thread::sleep(KEY_DELAY);
            }
            3 => { self.press_key(KEY_Z); thread::sleep(KEY_DELAY); }   // CCW
            _ => {}
        }

        let key = if dx > 0 { KEY_RIGHT } else { KEY_LEFT };
        for _ in 0..dx.unsigned_abs() {
            self.press_key(key);
            thread::sleep(KEY_DELAY);
        }

        // Brief pause before hard drop
        thread::sleep(Duration::from_millis(15));
        self.press_key(KEY_SPACE); // hard drop
        true
    }
}

impl Drop for VirtualKeyboard {
    fn drop(&mut self) {
        unsafe {
            libc::ioctl(self.file.as_raw_fd(), ui_dev_destroy());
        }
        eprintln!("[Input] Virtual keyboard destroyed");
    }
}
