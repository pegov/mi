use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{Sender, channel},
    },
    thread::JoinHandle,
    time::Duration,
};

use crossterm::{
    cursor::{MoveDown, MoveToColumn, MoveUp, RestorePosition, SavePosition},
    execute,
    style::Print,
    terminal::{self, Clear, ClearType},
};

#[derive(Debug)]
pub enum Event {
    Token(String),
    Cursor(String),
    Stop,
}

pub struct TokenStream {
    stop: Arc<AtomicBool>,
    tx: Sender<Event>,
}

impl TokenStream {
    pub fn start(&mut self) {
        self.send_string("\n");
        std::thread::sleep(Duration::from_millis(500));
        self.send_string("1");
        std::thread::sleep(Duration::from_millis(500));
        self.send_string("a");
        std::thread::sleep(Duration::from_millis(500));
        self.send_string("\n");
        std::thread::sleep(Duration::from_millis(500));
        std::thread::sleep(Duration::from_millis(500));
        self.send_string("4");
        std::thread::sleep(Duration::from_millis(500));
        for _ in 1..3 {
            self.tx.send(Event::Token(String::from("\n"))).unwrap();
            std::thread::sleep(Duration::from_millis(500));
            for i in 1..5 {
                self.tx.send(Event::Token(format!("{i}"))).unwrap();
                std::thread::sleep(Duration::from_millis(500));
            }
            self.tx.send(Event::Token(String::from("\n"))).unwrap();
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    fn send_string(&mut self, s: &str) {
        self.tx.send(Event::Token(String::from(s))).unwrap();
    }

    pub fn stop(&mut self) {
        self.tx.send(Event::Stop).unwrap();
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub struct CursorStream {
    stop: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    spinner_idx: Arc<AtomicUsize>,
    tx: Sender<Event>,
    handle: Option<JoinHandle<()>>,
}

pub const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

impl CursorStream {
    pub fn start(&mut self) {
        self.running.store(true, Ordering::Relaxed);
        let running_clone = Arc::clone(&self.running);
        let spinner_idx_clone = Arc::clone(&self.spinner_idx);
        self.handle = Some(std::thread::spawn(move || {
            loop {
                if !running_clone.load(Ordering::Relaxed) {
                    break;
                }

                spinner_idx_clone.update(Ordering::Relaxed, Ordering::Relaxed, |curr| {
                    (curr + 1) % SPINNER.len()
                });

                std::thread::sleep(Duration::from_millis(50));
            }
        }));

        loop {
            let c = SPINNER[self.spinner_idx.load(Ordering::Relaxed)];
            self.tx.send(Event::Cursor(c.to_string())).unwrap();

            if self.stop.load(Ordering::Relaxed) {
                self.running.store(false, Ordering::Relaxed);
                if let Some(handle) = self.handle.take() {
                    handle.join().unwrap();
                }
                return;
            }

            std::thread::sleep(Duration::from_millis(50));
        }
    }

    pub fn stop(&mut self) {
        // TODO
    }
}

pub fn chan() -> anyhow::Result<()> {
    let (tx, rx) = channel::<Event>();

    let stop = Arc::new(AtomicBool::new(false));
    let mut ts = TokenStream {
        stop: Arc::clone(&stop),
        tx: tx.clone(),
    };
    let mut cs = CursorStream {
        stop: Arc::clone(&stop),
        tx: tx.clone(),
        handle: None,
        running: Arc::new(AtomicBool::new(false)),
        spinner_idx: Arc::new(AtomicUsize::new(0)),
    };

    let ts_handle = std::thread::spawn(move || {
        ts.start();
        ts.stop();
    });

    let cs_handle = std::thread::spawn(move || {
        cs.start();
        cs.stop();
    });

    let mut stdout = std::io::stdout();

    terminal::enable_raw_mode()?;

    execute!(stdout, Print("\r\n"))?;
    execute!(stdout, MoveUp(1), MoveToColumn(0))?;
    execute!(stdout, SavePosition)?;

    let mut g_spinner = String::new();

    while let Ok(event) = rx.recv() {
        match event {
            Event::Token(token) => {
                for c in token.chars() {
                    if c == '\n' {
                        execute!(
                            stdout,
                            Print("\r\n"),
                            Clear(ClearType::CurrentLine),
                            Print("\r\n"),
                            MoveUp(1),
                            MoveToColumn(0)
                        )?;
                        execute!(stdout, SavePosition)?;
                        execute!(stdout, RestorePosition)?;
                        execute!(stdout, MoveDown(1), MoveToColumn(0))?;
                        execute!(stdout, Print(format!("{g_spinner} STREAMING...")))?;
                        execute!(stdout, RestorePosition)?;
                    } else {
                        execute!(stdout, Print(c))?;
                    }
                    execute!(stdout, SavePosition)?;
                }
            }
            Event::Cursor(spinner) => {
                g_spinner = spinner.clone();
                execute!(stdout, RestorePosition)?;
                execute!(stdout, MoveDown(1), MoveToColumn(0))?;
                execute!(stdout, Print(format!("{spinner} STREAMING...")))?;
                execute!(stdout, RestorePosition)?;
            }
            Event::Stop => {
                execute!(
                    stdout,
                    RestorePosition,
                    MoveDown(1),
                    MoveToColumn(0),
                    Clear(ClearType::CurrentLine),
                    Print("STOP\r\n"),
                )?;

                // stdout.flush()?;
                stop.store(true, Ordering::Relaxed);
                break;
            }
        }
    }

    ts_handle.join().unwrap();
    cs_handle.join().unwrap();

    terminal::disable_raw_mode()?;

    Ok(())
}
