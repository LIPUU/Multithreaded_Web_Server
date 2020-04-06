use std::thread::{JoinHandle, spawn};
use std::sync::mpsc::{Sender, Receiver, channel};
use std::sync::{Arc, Mutex};

pub struct ThreadPool {
    workers: Vec<Worker>,
    sender: Sender<Message>,
}
/*
impl Drop for ThreadPool {
    fn drop(&mut self) {
        for worker in &mut self.workers {
            println!("Shutting down worker{}", worker.id);

            // 这么写是错误的,因为我们拥有的是引用而并非所有权,但join要求拿到JoinHandle<()>的ownership
            // worker.thread.join().unwrap();

            // 可以加上一层来完成这个操作.如果Worker存放的是Option<thread::JoinHandle<()>>,就可以在Option上调用take方法将值从Some成员中移动出来
            // 而take方法对None不做处理.也就是说,正在运行的Worker的thread将是Some成员值,而当需要清理worker时,将Some替换成None,
            // 这样worker就没有可以运行的线程了. 所以要更新Worker的定义,更新完之后可以继续实现Drop trait

            // 如果里面有线程,那就把线程取出来(取线程的操作在语义上实际上是取得线程的JoinHandle句柄),然后原地就剩下了None
            // 如果没线程,那就证明原来就是None.
            if let Some(thread) = worker.thread.take() {
                // 但实际上这么做是不正确的,调用join()并不会关闭线程,因为他们一直在loop期待新任务.如果使用这个实现作为Drop的最终实现,则主线程
                // 将永远阻塞在等待第一个线程结束.
                thread.join().unwrap();
            }
        }
    }
}
*/

/* 正确版本的Drop */
impl Drop for ThreadPool {
    fn drop(&mut self) {
        println!("{}","Start Droping... Sending terminate message to all workers.");

        // 发送端其实只有一个,那就是ThreadPool里面的Sender,"响应浏览器请求"的handle_connection由execute发送给线程,而终止信号由本Drop发送.
        // 准备停机之前用for挨个发送Terminate信号告诉worker退出loop.如果有一个线程还没有执行完怎么办? 根据代码的设计,线程在执行闭包的时候是不持有锁的,
        // 所以这个Terminate一定会由其他的某个正阻塞于loop中lock()的线程获取(只有拿到了lock才能够执行receiver的recv操作).
        // 又因为for执行的次数和worker的数量相等,所以无论线程们当前是什么状态(阻塞在lock上,或者是正在执行闭包中),
        // 最终每一个线程都一定能够收到Terminate,只不过是有先后次序罢了.
        for _ in &mut self.workers {
            self.sender.send(Message::Terminate).unwrap()
        }
        // 发终止信号过程中再有新任务进来怎么办? 关键在于:是只有在执行ThreadPool的Drop时候才会发送终止信号,
        // 也就是说,main中的ThreadPool的实例对象pool的生命周期结束的时候才能够发送终止信号,
        // 那实际上意味着,main中的for stream in listener.incoming() {循环体} 因为什么原因而结束运行了,导致pool不再被使用从而be killed
        // 由于循环的结束,导致不会有stream再去遍历"请求尝试"了,所有在浏览器发出的新请求都会被拒绝,就好像从未有程序监听该端口一样,也就是说,只要开始发送终止信号
        // 就再也不会有新任务能够被发送进来了.

        // 如果没有这个join,则导致main中在for stream in listener.incoming().take(2) 结束时候,主进程立即结束,从而所有线程都立即结束
        // 如果某些线程还没有把请求执行完毕就被迫结束了,浏览器那里将会立即被中断,这不够优雅.
        // 而有了本join,配合上上面的Terminate,使得停机非常优雅:不再接受新请求了,但是如果现有的请求还没执行完,那么就把它从Some里面用take取出来并
        // 等待它执行完毕( join() ).
        println!("{}","Shutting down all workers.");
        for worker in &mut self.workers {
            println!("Shutting down worker {}",worker.id);
            if let Some(thread)=worker.thread.take(){
                thread.join().unwrap();
            }
        }

/*
为什么这里要走两遍for,而不是一次for里做两件事呢(先发送终止信号然后再join()等待)? 假如我们的Workers的长度为2,worker1的线程1正在执行闭包中,
那么显然发送的Terminate信号会被线程2获取,于是线程2退出loop,再也不会尝试lock.recv接受新任务.然后本for接下来尝试对worker1的线程1进行join(),
因为join()干的事情是等待被join()的线程执行结束,但这里join绝对完不成这个任务,因为被join()的线程1还没有接收到Terminate,从而一直在执行loop,也就是说
它接下来将执行lock且能够成功,而本Drop之所以能够执行实际上意味着浏览器那边将没有有任何请求能够发过来了,也就是说lock()成功之后,接下来执行的recv()拿到的绝对不会
是内含handle_connection的闭包.又因为本次for已经把Terminate发送给了线程2然后join等待线程1执行,这导致线程1的recv()也拿不到Terminate然后退出loop.此时线程1
卡在loop里的recv()拿不到Message,DeadLock,Bingo! 所以要把发送Terminate的操作(线程依然还开着,Some持有它的句柄)和等待线程执行完的join操作分成两轮.
这样能够确保所有线程都能收到Terminate从而退出loop且能够被等待执行完毕.
*/

    }
}


// 为了修复程序阻塞在"等待第一个线程的结束",线程除了监听是否有Job运行之外,也要监听一个表明"应该停止监听并退出无限循环loop"的信号.
// 所以可以在发送Job的逻辑处下手,给Job包上一层枚举,这样发送的时候Job和该枚举其他的项都是同种类型,在接收处使用match进行匹配具体是Job还是别的项
enum Message {
    // 这里并没有进行大刀阔斧的修改,如直接把Job挪到这里面来,而是直接用一个单元组把Job包起来,这样可以尽可能少地减少修改
    NewJob(Job),
    Terminate,
}
// 由此处定义可以看出来Message要么是存放了线程需要运行的任务Job的NewJob成员,要么是会导致线程退出循环并终止的Terminate成员


// Box<dyn 的组合用来指代一系列实现了特定Trait的闭包类型,type给冗长的类型加上了别名
type Job = Box<dyn FnOnce() + Send + 'static>;

// 用Worker把空线程句柄包一层可以实现记录id的效果,调试的时候可以打印到底是哪个线程执行了任务.
struct Worker {
    id: usize,
    // JoinHandle<T>类型可以用来保存线程
    // spawn的签名为: fn spawn(F)->JoinHandle<T> where F: FnOnce()->T +'static+Send ,T:'static+Send
    // 其中,T是F的返回值.在本例中闭包F的返回值取决于闭包内执行的handle_connection()->()函数,所以最终导致了JoinHandle<()>
    thread: Option<JoinHandle<()>>,
}


impl Worker {
    fn new(id: usize, receiver: Arc<Mutex<Receiver<Message>>>) -> Worker {
        Worker {
            id,
            thread: Some(spawn(move || {
                loop {
                    let message = receiver.lock().unwrap().recv().unwrap();
                    match message {
                        Message::NewJob(job) => {
                            println!("Worker{} got a job.", id);
                            job()
                        },
                        Message::Terminate => {
                            println!("Worker {} was told to terminate.", id);
                            break;
                        }
                    }
                }

            })),
        }
    }
}

impl ThreadPool {
    pub fn new(size: usize) -> ThreadPool {
        assert!(size > 0);

        // spawn生成完新线程之后立即执行代码,但是我们希望能够在生成线程之后暂且不执行代码,直到有任务发送给线程
        // 这需要引入一个新的结构:Worker
        let mut workers = Vec::with_capacity(size);

        let (sender, receiver) = channel();

        let receiver = Arc::new(Mutex::new(receiver));
        for i in 0..size {
            workers.push(Worker::new(i, receiver.clone()));
        }

        ThreadPool {
            workers,
            sender,
        }
    }

    /// 模仿thread::spawn给闭包加上种类的限定.
    /// 线程池里面最终还是要开辟线程,所以要把execute里面的参数传递给线程来执行,且一个闭包只会执行一次,所以本着最小化的原则闭包类型为FnOnce()
    /// 闭包交付给线程执行的时候其实就是在线程间转移闭包的所有权,所以使用Send进行限定.
    /// 用'static的原因是不知道线程能够执行多久
    pub fn execute<F>(&self, f: F)
        where F: 'static + Send + FnOnce() {
        // 因为每个闭包大小可能不一样,但是Job在定义的时候加上了dyn,也就是说是意图使用动态分配来存储闭包,所以最外面要加上Box包裹
        let job = Box::new(f);
        self.sender.send(Message::NewJob(job)).unwrap();
    }
}