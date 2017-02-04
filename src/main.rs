extern crate threadpool;
extern crate tasks_framework;
extern crate regex;
extern crate num_cpus;
extern crate nix;

use std::io;
use std::io::prelude::*;

use std::io::BufWriter;
use std::cell::RefCell;

use std::process;
use std::env;

use regex::Regex;
use nix::sys::signal;
use nix::sys::signal::SigHandler::Handler;

use tasks_framework::single_queue_actor::MessageProcessor;
use tasks_framework::single_queue_actor::SingleQueueActor;

use threadpool::ThreadPool;
use std::sync::Arc;

struct LineProcessor {
    regex: Regex,
    buf_writer: RefCell<BufWriter<io::Stdout>>
}

unsafe impl Send for LineProcessor {}
unsafe impl Sync for LineProcessor {}

impl LineProcessor {
    fn new(regex_string: String) -> LineProcessor {
        LineProcessor {
            regex: Regex::new(regex_string.as_str()).unwrap(),
            buf_writer: RefCell::new(BufWriter::with_capacity(8192, io::stdout()))
        }
    }
}

impl MessageProcessor<String> for LineProcessor {
    fn process_message(&self, line: String) {
        let maybe_caps = self.regex.captures(line.as_str());
        maybe_caps.map(|caps| if caps.len() > 1 {
            let mut cap_vec: Vec<&str> = Vec::with_capacity(caps.len()-1);
            for i in 1..caps.len() {
                cap_vec.push(caps.get(i).map(|m| m.as_str()).unwrap_or("[NONE]"));
            }
            let _ = self.buf_writer.borrow_mut().write_all(format!("{}\n", cap_vec.join("\t")).as_bytes());
        } else {
            let _ = self.buf_writer.borrow_mut().write_all(format!("{}\n", caps.get(0).unwrap().as_str()).as_bytes());
        });
    }
}

extern fn handle_sig(_:i32) {
    process::exit(0);
}

fn main() {
    let sig_action = signal::SigAction::new(Handler(handle_sig),
                                            signal::SaFlags::empty(),
                                            signal::SigSet::empty());
    unsafe {
        let _ = signal::sigaction(signal::SIGINT, &sig_action);
        let _ = signal::sigaction(signal::SIGPIPE, &sig_action);
    }

    let pattern = env::args().nth(1);
    if pattern.is_none() {
        panic!("No pattern given");
    }

    let cpus = num_cpus::get();

    let pool = Arc::new(ThreadPool::new(cpus));
    let processor = Arc::new(LineProcessor::new(pattern.unwrap().to_owned()));

    let mut actors: Vec<SingleQueueActor<String, LineProcessor>> = vec!();
    for _ in 0..(cpus*4) {
        let simple_actor = SingleQueueActor::new( processor.clone(), pool.clone() );
        actors.push(simple_actor);
    }

    let mut rr_index = 0;

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        actors.get(rr_index).map(|ref a| a.add_message(line.unwrap()));
        rr_index = (rr_index + 1) % actors.len();
    }

    for actor in actors.iter() {
        actor.complete();
    }
}
