use std::cell::RefCell;
use libc::pid_t;
use litemap::LiteMap;
use std::rc::{Rc, Weak};

#[derive(Debug)]
pub(crate) struct ProcessInfo {
    pid: pid_t,
    parent: Weak<RefCell<ProcessInfo>>,
    children: LiteMap<pid_t, Rc<RefCell<ProcessInfo>>>,
    whole_tree_info: Rc<RefCell<LiteMap<pid_t, Weak<RefCell<ProcessInfo>>>>>,
}

impl ProcessInfo {
    pub(crate) fn new(pid: pid_t) -> Rc<RefCell<Self>> {
        let result = Rc::new(RefCell::new(ProcessInfo {
            pid,
            parent: Weak::new(),
            children: LiteMap::new_vec(),
            whole_tree_info: Rc::new(RefCell::new(LiteMap::new_vec())),
        }));

        assert!(result.borrow_mut().whole_tree_info.borrow_mut().insert(pid, Rc::downgrade(&result)).is_none());

        result
    }

    pub(crate) fn get_pid(&self) -> pid_t {
        self.pid
    }

    pub(crate) fn get_parent(&self) -> Option<Rc<RefCell<ProcessInfo>>> {
        self.parent.upgrade()
    }

    pub(crate) fn add_child(&mut self, child_pid: pid_t) -> Rc<RefCell<ProcessInfo>> {
        let child = Rc::new(RefCell::new(
            ProcessInfo {
                pid: child_pid,
                parent: self.whole_tree_info.borrow().get(&self.pid)
                    .expect("self.pid not found in whole_tree_info").clone(),
                children: LiteMap::new_vec(),
                whole_tree_info: self.whole_tree_info.clone(),
            }
        ));

        assert!(self.children.insert(child_pid, child.clone()).is_none());
        assert!(self.whole_tree_info.borrow_mut().insert(child_pid, Rc::downgrade(&child)).is_none());

        child
    }

    pub(crate) fn delete_child(&mut self, child_pid: pid_t) {
        assert!(self.children.remove(&child_pid).is_some());
        assert!(self.whole_tree_info.borrow_mut().remove(&child_pid).is_some());
    }

    pub(crate) fn get_child(&self, child_pid: pid_t) -> Option<Rc<RefCell<ProcessInfo>>> {
        self.children.get(&child_pid).cloned()
    }

    pub(crate) fn get_process(&mut self, child_pid: pid_t) -> Option<Rc<RefCell<ProcessInfo>>> {
        if let Some(process) = self.whole_tree_info.borrow().get(&child_pid) {
            if let Some(upgraded) = process.upgrade() {
                Some(upgraded)
            } else {
                self.whole_tree_info.borrow_mut().remove(&child_pid);
                None
            }
        } else {
            None
        }
    }
}