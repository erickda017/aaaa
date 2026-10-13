//! Atalho global (funciona com o Discord/jogo em primeiro plano).
//!
//! No Windows usamos polling de `GetAsyncKeyState` a ~125 Hz: não precisa de hook nem
//! de loop de mensagens, e aceita teclado e botões laterais do mouse.

use crate::shared::Shared;
use std::sync::Arc;

/// Teclas oferecidas na interface: (código virtual do Windows, nome).
pub const KEY_CHOICES: &[(u32, &str)] = &[
    (0x70, "F1"),
    (0x71, "F2"),
    (0x72, "F3"),
    (0x73, "F4"),
    (0x74, "F5"),
    (0x75, "F6"),
    (0x76, "F7"),
    (0x77, "F8"),
    (0x78, "F9"),
    (0x79, "F10"),
    (0x7A, "F11"),
    (0x7B, "F12"),
    (0x2D, "Insert"),
    (0x24, "Home"),
    (0x23, "End"),
    (0x21, "Page Up"),
    (0x22, "Page Down"),
    (0x91, "Scroll Lock"),
    (0x13, "Pause"),
    (0x60, "Num 0"),
    (0x61, "Num 1"),
    (0x62, "Num 2"),
    (0x63, "Num 3"),
    (0x64, "Num 4"),
    (0x65, "Num 5"),
    (0x66, "Num 6"),
    (0x67, "Num 7"),
    (0x68, "Num 8"),
    (0x69, "Num 9"),
    (0x6A, "Num *"),
    (0x6B, "Num +"),
    (0x6D, "Num -"),
    (0x6E, "Num ,"),
    (0x6F, "Num /"),
    (0x05, "Mouse lateral 1 (voltar)"),
    (0x06, "Mouse lateral 2 (avançar)"),
    (0x04, "Mouse botão do meio"),
    (0xC0, "` (acento grave)"),
    (0xA2, "Ctrl esquerdo"),
    (0xA3, "Ctrl direito"),
    (0xA0, "Shift esquerdo"),
    (0xA4, "Alt esquerdo"),
];

pub fn key_name(vk: u32) -> String {
    KEY_CHOICES
        .iter()
        .find(|(v, _)| *v == vk)
        .map(|(_, n)| n.to_string())
        .unwrap_or_else(|| format!("Tecla 0x{vk:02X}"))
}

#[cfg(windows)]
pub fn spawn(shared: Arc<Shared>, ctx: eframe::egui::Context) {
    use std::sync::atomic::Ordering::Relaxed;
    use std::time::Duration;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

    fn down(vk: u32) -> bool {
        // bit mais significativo = tecla pressionada agora
        unsafe { (GetAsyncKeyState(vk as i32) as u16) & 0x8000 != 0 }
    }

    std::thread::Builder::new()
        .name("hotkey".into())
        .spawn(move || {
            let mut was_down = false;
            loop {
                std::thread::sleep(Duration::from_millis(8));

                if shared.capture_request.load(Relaxed) {
                    // captura a próxima tecla pressionada (ignora botões esquerdo/direito do mouse)
                    if let Some(vk) = (3u32..=254).find(|&vk| vk != 1 && vk != 2 && down(vk)) {
                        shared.hotkey_vk.store(vk, Relaxed);
                        shared.capture_request.store(false, Relaxed);
                        was_down = true; // não dispara a troca com a mesma pressão
                        ctx.request_repaint();
                    }
                    continue;
                }

                let pressed = down(shared.hotkey_vk.load(Relaxed));
                if shared.hotkey_hold.load(Relaxed) {
                    if pressed != shared.is_active() {
                        shared.set_active(pressed);
                        ctx.request_repaint();
                    }
                } else if pressed && !was_down {
                    shared.toggle_active();
                    ctx.request_repaint();
                }
                was_down = pressed;
            }
        })
        .expect("thread do atalho");
}

/// Fora do Windows (desenvolvimento) não há atalho global.
#[cfg(not(windows))]
pub fn spawn(_shared: Arc<Shared>, _ctx: eframe::egui::Context) {}
