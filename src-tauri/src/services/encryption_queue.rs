use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Manager};

use crate::database::DbState;

#[derive(Clone, Copy, Debug)]
pub enum EncryptionAction {
    Encrypt,
    Decrypt,
}

#[derive(Clone, Copy, Debug)]
pub struct EncryptionJob {
    pub id: i64,
    pub action: EncryptionAction,
}

#[derive(Clone)]
pub struct EncryptionQueue {
    sender: Sender<EncryptionJob>,
}

impl EncryptionQueue {
    pub fn enqueue(&self, job: EncryptionJob) {
        let _ = self.sender.send(job);
    }
}

pub fn init_encryption_queue(app_handle: AppHandle) -> EncryptionQueue {
    let (tx, rx) = mpsc::channel::<EncryptionJob>();
    thread::spawn(move || worker(app_handle, rx));
    EncryptionQueue { sender: tx }
}

fn worker(app_handle: AppHandle, rx: Receiver<EncryptionJob>) {
    while let Ok(job) = rx.recv() {
        let mut jobs = vec![job];
        while let Ok(next) = rx.try_recv() {
            jobs.push(next);
        }

        for job in jobs {
            let db_state = app_handle.state::<DbState>();
            let conn = match db_state.conn.lock() {
                Ok(c) => c,
                Err(_) => {
                    thread::sleep(Duration::from_millis(30));
                    continue;
                }
            };

            let result = match job.action {
                EncryptionAction::Encrypt => db_state.repo.encrypt_entry_with_conn(&conn, job.id),
                EncryptionAction::Decrypt => db_state.repo.decrypt_entry_with_conn(&conn, job.id),
            };

            if let Err(err) = result {
                eprintln!("encryption queue job failed (id={}): {}", job.id, err);
            }

            drop(conn);
            thread::sleep(Duration::from_millis(30));
        }
    }
}
