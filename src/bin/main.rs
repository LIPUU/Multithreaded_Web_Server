use std::net::TcpStream;
use std::net::TcpListener;
use std::io::prelude::*;
use std::fs;
use std::fs::read_to_string;
use std::thread;
use std::time::Duration;
use hello::ThreadPool;

fn main() {
    let listener = TcpListener::bind("127.0.0.1:7878").unwrap();

    // incoming()返回的迭代器提供了一系列的流,流代表客户端和服务器端连接的全部过程,可以理解成套接字.
    // 遍历的并非"连接",而是"连接尝试".连接也不能够被遍历.

    // 启动线程池
    let pool=ThreadPool::new(4);

    // 其实程序一直是阻塞在这里的
    for stream in listener.incoming() {
        let stream = stream.unwrap();
        // 获取到连接尝试之后,用专用函数对其进行解析. 把"调用解析函数"这一操作用闭包传递给池子来执行,由池子决定何时处理哪个任务.
        // 想要完成这个任务需要进行一些代码上的设计,设计成这样的原因是:
        // 1.避免只是简单地给每一个任务都开辟新线程
        // 2.接口设计成pool.execute(xxx)是为了模仿💠开辟线程的语法thread::spawn
        // 3.传递闭包而不是函数是因为闭包能够捕获环境中的变量stream,而函数不能捕获(更加深刻体现了闭包就是函数+环境变量),即使它们都有延时被调用的能力.
        pool.execute(|| {
            handle_connection(stream);
        });
        // 既然设计了闭包,那就要确定要使用哪种闭包,i.e.给闭包加上一些限定,他们限定的就是闭包的种类.
        // 这些限定包括:FnOnce/Fn/FnMut(三种闭包) , Send/Sync(闭包实现的Trait) , 'static (lifetime) 等
    }

    // 对TcpStream的读取需要mut关键字
    fn handle_connection(mut stream: TcpStream) {
        let mut buffer = [0; 512];
        stream.read(&mut buffer).unwrap();

        // 如果我们想要把请求搞到String里面去,那么必须用String::from_utf8_lossy
        // &s[..]最终拿到的是&[u8],它把无效的UTF-8序列以�代替,
        // 这样请求就被从buffer搞到
        let request = String::from_utf8_lossy(&buffer[..]);
        println!("{}", request);

        // 加b直接把&str以&[u8]的形式存储
        let get = b"GET / HTTP/1.1\r\n";
        let sleep = b"GET /sleep HTTP/1.1\r\n";

        let (status_line, filename) = if buffer.starts_with(get) {
            ("HTTP/1.1 200 OK\r\n\r\n", "hello.html")
        } else if buffer.starts_with(sleep) {
            thread::sleep(Duration::from_secs(5));
            ("HTTP/1.1 200 OK\r\n\r\n", "hello.html")
        } else {
            ("HTTP/1.1 404 NOT FOUND\r\n\r\n", "404.html")
        };
        let contents = fs::read_to_string(filename).unwrap();
        let response = format!("{}{}", status_line, contents);
        stream.write(response.as_bytes()).unwrap();
        stream.flush().unwrap();
    }
}
