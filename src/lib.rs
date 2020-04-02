use std::thread::{JoinHandle, spawn};

pub struct ThreadPool {
    workers: Vec<Worker>
}

// 用Worker把空线程句柄包一层可以实现记录id的效果,调试的时候可以打印到底是哪个线程执行了任务.
struct Worker {
    id: usize,
    // JoinHandle<T>类型可以用来保存线程
    // spawn的签名为: fn spawn(F)->JoinHandle<T> where F: FnOnce()->T +'static+Send ,T:'static+Send
    // 其中,T是F的返回值.在本例中闭包F的返回值取决于闭包内执行的handle_connection()->()函数,所以最终导致了JoinHandle<()>
    thread: JoinHandle<()>,
}

impl Worker {
    fn new(id: usize) -> Worker {
        Worker {
            // 每一个Worker结构体都有一个id,和一个空线程
            id,
            thread: spawn(move || {}),
        }
    }
}

impl ThreadPool {
    pub fn new(size: usize) -> ThreadPool {
        assert!(size > 0);

        // spawn生成完新线程之后立即执行代码,但是我们希望能够在生成线程之后暂且不执行代码,直到有任务发送给线程
        // 这需要引入一个新的结构:Worker
        let mut workers = Vec::with_capacity(size);

        for i in 0..size {
            workers.push(Worker::new(i));
        }

        ThreadPool {
            workers
        }
    }

    /// 模仿thread::spawn给闭包加上种类的限定.
    /// 线程池里面最终还是要开辟线程,所以要把execute里面的参数传递给线程来执行,且一个闭包只会执行一次,所以本着最小化的原则闭包类型为FnOnce()
    /// 闭包交付给线程执行的时候其实就是在线程间转移闭包的所有权,所以使用Send进行限定.
    /// 用'static的原因是不知道线程能够执行多久
    pub fn execute<F>(&self, f: F)
        where F: 'static + Send + FnOnce() {

    }
}