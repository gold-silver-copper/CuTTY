use std::{
    io::{Read, Write},
    sync::{Arc, Mutex, mpsc},
    thread,
};

use anyhow::{Context, Result};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use winit::event_loop::EventLoopProxy;

use crate::events::UserEvent;

pub struct PtyProcess {
    master: Box<dyn MasterPty + Send>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    reader_rx: mpsc::Receiver<Vec<u8>>,
    _child: Box<dyn Child + Send + Sync>,
}

impl PtyProcess {
    pub fn spawn(proxy: EventLoopProxy<UserEvent>, cols: u16, rows: u16) -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("failed to open PTY")?;

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_owned());
        let mut cmd = CommandBuilder::new(&shell);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");

        let child = pair
            .slave
            .spawn_command(cmd)
            .context("failed to spawn shell")?;
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .context("failed to clone PTY reader")?;
        let writer = Arc::new(Mutex::new(
            pair.master
                .take_writer()
                .context("failed to take PTY writer")?,
        ));
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            let mut buf = [0_u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        let _ = proxy.send_event(UserEvent::PtyExit);
                        break;
                    }
                    Ok(read) => {
                        if tx.send(buf[..read].to_vec()).is_err() {
                            break;
                        }
                        let _ = proxy.send_event(UserEvent::PtyUpdate);
                    }
                    Err(_) => {
                        let _ = proxy.send_event(UserEvent::PtyExit);
                        break;
                    }
                }
            }
        });

        Ok(Self {
            master: pair.master,
            writer,
            reader_rx: rx,
            _child: child,
        })
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
    }

    pub fn write_all(&self, bytes: &[u8]) {
        if let Ok(mut writer) = self.writer.lock() {
            let _ = writer.write_all(bytes);
            let _ = writer.flush();
        }
    }

    pub fn drain(&self) -> Vec<Vec<u8>> {
        let mut chunks = Vec::new();
        while let Ok(bytes) = self.reader_rx.try_recv() {
            chunks.push(bytes);
        }
        chunks
    }
}
