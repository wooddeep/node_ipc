# node_ipc
ipc for nodejs base on share memory and semaphore!

# description
  The default IPC mechanism of Node.js does not support direct communication between worker processes, only communication between the master process and worker processes. Communication between worker processes requires intermediate transfer through the master process. The purpose of developing this project is to implement IPC communication between worker processes in Node.js cluster mode through napi-rs. IPC between processes is achieved through shared memory and semaphores. Currently, there are no open-source Node.js plugins available on the Internet, so I'm implementing one myself.  

  Currently, only direct communication between Node.js worker processes under Windows has been implemented. Subsequently, direct communication between worker processes under Linux and macOS will be gradually implemented.  

# usage 
1. prepare
- make a new directory, such as D:\ipc_test
- change the current directory to D:\ipc_test
- download and save the lib: npm i @wooddeep/node_ipc 
2. create the main entry file such as test.js, and complete it. this file start the node cluster mode with two worker processes, and each worker can work as a websocket server:
```javascript
let backend = require("@wooddeep/node_ipc")

const cluster = require("cluster")
const Router = require('koa-router')
const Koa = require('koa')
const bodyParser = require('koa-bodyparser')


const wsd = require("./wsd")
const events = require("events")

const app = new Koa()
let router = new Router()
app.use(bodyParser())

router.get('/require', async (ctx) => {
    await backend.testSemaRequire()
    ctx.body = 'testSemaRequire response'
});

router.get('/release', async (ctx) => {
    await backend.testSemaRelease()
    ctx.body = 'testSemaRelease response'
});

router.get('/read', async (ctx) => {
    backend.testShmRead()
    ctx.body = 'testShmRead response'
});

//  curl -H "Content-Type:application/json" -X POST http://127.0.0.1:5050/write -d '{"key": "val"}'
router.post('/write', async (ctx) => {
    await backend.testShmWrite(JSON.stringify(ctx.request.body))
    ctx.body = 'testShmWrite response'
})

// 加载路由中间件
app.use(router.routes()).use(router.allowedMethods())

const child_proc_num = 2 // /*os.cpus().length*/

process.on("SIGINT", () => {
    backend.processExit()
    process.exit()
});

process.on("beforeExit", (code) => {
    console.log("## pre exit in node...")
    backend.processExit()
})

function delay(secs) {
    let promise = new Promise((resolve) => {
        setTimeout(() => resolve(true), 1000 * secs)
    });
    return promise;
}

function do_sub() {
    subscribe(async () => {
        let data = await backend.testShmRead();
        if (data.length > 2) {
            console.log(`## process id: ${process.pid}; data.length = ${data.length}, data = ${data}, time = ${new Date()}`)
        }
    });
}

function subscribe(callback) {
    backend.callNodeFunc().then((data) => {
        callback(data)
        subscribe(callback)
    })
}

if (cluster.isMaster) { // main process
    cluster.schedulingPolicy = cluster.SCHED_RR;
    backend.masterInit(child_proc_num) // 

    let child_map = new Map()
    for (var i = 0, n = child_proc_num ; i < n; i += 1) {
        let new_worker_env = {};
        new_worker_env["WORKER_INDEX"] = i;
        let child = cluster.fork(new_worker_env); // start child process
        child_map.set(child.process.pid, i)
    }

    cluster.on("exit", (worker, code, signal) => { // start again when one child exit!
        let new_worker_env = {};
        let index = child_map.get(worker.process.pid)
        new_worker_env["WORKER_INDEX"] = index;
        child_map.delete(worker.process.pid)
        let child = cluster.fork(new_worker_env);
        child_map.set(child.process.pid, index)
    })

} else {
    backend.workerInit(child_proc_num, Number.parseInt(process.env["WORKER_INDEX"]))
    const emitter = new events.EventEmitter();
    backend.callSafeFunc(async (data) => {
        if (data.length > 2) {
            console.log(`## process id: ${process.pid}; data.length = ${data.length}, data = ${data}, time = ${new Date()}`)
            emitter.emit("peer", data)
        }
    });

    process.WORKER_INDEX = process.env["WORKER_INDEX"]
    console.log("WORKER_INDEX", process.env["WORKER_INDEX"])

    const websockd = new wsd(emitter);

    websockd.start(emitter);
    
}

```

3. create the web socket main file such as wsd.js, which used for managing WebSocket connections in the worker process, including establishing connections, disconnecting, and receiving data. On the other hand, the constructor takes an event emitter object as input, which is used to listen for messages sent by other worker processes, thus enabling direct communication between worker processes.
```javascript
const WebSocket = require('ws')
const backend = require("@wooddeep/node_ipc")

class WsServer {

    constructor(emitter) {
        this.connMap = new Map()
        this.emitter = emitter
        this.emitter.on("peer", (msg) => {
            console.log("receive message:", msg)
            this.connMap.forEach((value, socket) => {
                socket.send(msg)
            })

        })
    }

    async onMessage(message, socket) {
        let data = JSON.parse(message.toString());
        //console.log(`## onMessage: ${JSON.stringify(data)}`)
        await backend.testShmWrite(JSON.stringify(JSON.stringify(data)))
    }

    onClose(socket) {
        console.log(`## onClose`)
        this.connMap.delete(socket)
    }

    onError(socket, error) {
        console.log(`## onError: ${error}`)
        this.connMap.delete(socket)
    }

    async onConnect(socket, req) {
        socket.on("message", async (message) => {
            await this.onMessage(message, socket)
        });
        socket.on("close", () => this.onClose(socket));
        socket.on("error", (error) => this.onError(socket, error));
        this.connMap.set(socket, true)

    }

    start() {
        this.server = new WebSocket.Server({port: 5050, path: "/test"})
        this.server.on('connection', async (socket, req) => {
            await this.onConnect(socket, req)
        });
    }

    stop() {
        if (this.server) {
            this.server.close();
        }
    }
}

module.exports = WsServer
```

4. create a websocket test client file wsc.js, which establish websocket connection with wsd.js, and send data to websocket server periodically.
```javascript
const WebSocket = require('ws');
var zlib = require('zlib');

const msg = { "groupId": "0", "cat": 0, "cmd": 0, "ani": 0, "pos": { "x": 0.0, "y": 0.0, "z": 0.0 }, "rot": { "x": 0.0, "y": 0.0, "z": 0.0 }, "id": "1" }

async function inflate(input) {
    var promise = new Promise((resolve) => {
        zlib.inflate(input, function (err, buf) {
            resolve(buf)
        });
    })

    return promise
}

class TestClient {
    constructor(url, groupId = '0', uid, gx = 0, gz = 0) {
        this.url = url;
        this.ms_count = 0;
        this.is_created = false;
        this.ws = null;
        this.data = null;
        this.id = uid
        this.sendData = msg;
        this.groupId = groupId
        this.limit = 1000
        this.begin = 0
        this.step = 0
        this.userSet = new Set()
        this.connectSate = -1
        this.retry = 0
        this.others = [{ "id": uid }]
        this.gx = gx
        this.gz = gz
    }

    squareIm() {
        this.sendData.biz = 16;
        this.sendData.cmd = 0;
        this.sendData.msg = `msg: ${this.id}-${Date.now()}`
        this.sendData.id =  this.id; // 用户ID
        this.ws.send(JSON.stringify(this.sendData));
    }

    connect() {
        if (this.ws.readyState == WebSocket.OPEN) {
            this.sendData.cat = 0; // 创建
            this.sendData.biz = 0;
            this.sendData.id =  this.id; // 用户ID
            this.sendData.groupId =  this.groupId
            this.sendData.cst = new Date().getTime();
            this.sendData.localIp = "127.0.0.1"
            this.sendData.pos = msg.pos
            this.sendData.rot = msg.rot
            this.ws.send(JSON.stringify(this.sendData));
            this.connectSate = 0
        } else if (this.ws.readyState == WebSocket.CONNECTING) {
            console.warn('client connecting: %s', this.ws.readyState);
        } else {
            console.error('error client state: %s', this.ws.readyState);
        }

        this.update()
        setInterval(() => {
            //this.sendData.groupId = 200
            this.update()
        }, 3000)

        setInterval(() => {
            this.sendData.biz = 3;
            this.sendData.id =  this.id; // 用户ID
            this.ws.send(JSON.stringify(this.sendData));
        }, 3000)

        setInterval(() => {
            // this.squareIm()
        }, 3000)
    }

    update() {
        if (this.ws.readyState == WebSocket.OPEN) {
            this.sendData = {}
            this.sendData.cat = 2;
            this.sendData.biz = 0;
            this.sendData.cmd = 1;
            this.sendData.id = this.id; // 用户ID
            this.sendData.groupId = this.groupId
            this.sendData.cst = new Date().getTime();
            msg.pos.x = msg.pos.x + 0.1
            this.sendData.pos = msg.pos
            this.sendData.rot = msg.rot
            this.sendData.uuid = "xxxxxx"
            this.ws.send(JSON.stringify(this.sendData));

        } else if (this.ws.readyState == WebSocket.CONNECTING) {
            console.warn('client connecting: %s', this.ws.readyState);
        }
    }

    init() {
        this.ws = new WebSocket(this.url);
        console.log('connecting to server %', this.url);

        this.ws.on('open', () => {
            console.log('connected to server %', this.url);
            this.connect()
        });

        this.ws.on('close', () => {
            console.log('## channel closed! connect again!');
            this.connect()
        })

        this.ack = true
        this.ws.on('message', async (data) => {
            if (data[0] != 123 && data[0] != 91) { // 压缩后的消息， '{' 的编码是123
                //var base64Str = data.toString('base64')
                //console.log(base64Str)
                var rdata = await inflate(data)
                if (rdata != undefined) {
                    //console.log("------")
                    var object = JSON.parse(rdata.toString())
                    if ((object.length == null && object.biz == 3) || object.ack != null) {
                        console.log("[0]", rdata.toString())
                        return
                    }
                    console.log("------")
                    console.log("[1]", rdata.toString())
                    //let array = JSON.parse(rdata.toString())
                }
            } else {
                if (data.ack != null) {
                    return
                }

                console.log("------")
                console.log("[2]", data.toString())
            }

        });
    }

}

function startTest() {

    var groupId = process.argv[2]
    var userId = process.argv[3]

    var port = 5050 // websocket server's port which set at wsd.js
    if (process.argv.length > 4) {
        port = process.argv[4]
    }

    console.log(groupId, userId)

    var cli = new TestClient(`ws://127.0.0.1:${port}/test?debug=true&AUTH=abc&id=${userId}&name=${userId}`, groupId, userId)

    cli.init();
}

startTest();

```
5. start the websocket server:
```bash
node test.js
```
6. start two websocket client:
```bash
node wsc.js 100 100 # client id: 100
node wsc.js 100 101 # client id: 101
```
7. watch the two clients' output:
- client 100, get the client 101'1 message from other worker process:
```
$ node wsc.js 100 100
100 100
connecting to server % ws://127.0.0.1:5050/test?debug=true&AUTH=abc&id=100&name=100
connected to server % ws://127.0.0.1:5050/test?debug=true&AUTH=abc&id=100&name=100
------
[2] ["{\"groupId\":\"100\",\"cat\":0,\"cmd\":0,\"ani\":0,\"pos\":{\"x\":0,\"y\":0,\"z\":0},\"rot\":{\"x\":0,\"y\":0,\"z\":0},\"id\":\"101\",\"biz\":0,\"cst\":1682302513687,\"localIp\":\"127.0.0.1\"}","{\"cat\":2,\"biz\":0,\"cmd\":1,\"id\":\"101\",\"groupId\":\"100\",\"cst\":1682302513688,\"pos\":{\"x\":0.1,\"y\":0,\"z\":0},\"rot\":{\"x\":0,\"y\":0,\"z\":0},\"uuid\":\"xxxxxx\"}"]
------
[2] ["{\"cat\":2,\"biz\":0,\"cmd\":1,\"id\":\"101\",\"groupId\":\"100\",\"cst\":1682302516702,\"pos\":{\"x\":0.2,\"y\":0,\"z\":0},\"rot\":{\"x\":0,\"y\":0,\"z\":0},\"uuid\":\"xxxxxx\"}","{\"cat\":2,\"biz\":3,\"cmd\":1,\"id\":\"101\",\"groupId\":\"100\",\"cst\":1682302516702,\"pos\":{\"x\":0.2,\"y\":0,\"z\":0},\"rot\":{\"x\":0,\"y\":0,\"z\":0},\"uuid\":\"xxxxxx\"}"]
------
[2] ["{\"cat\":2,\"biz\":0,\"cmd\":1,\"id\":\"101\",\"groupId\":\"100\",\"cst\":1682302519717,\"pos\":{\"x\":0.30000000000000004,\"y\":0,\"z\":0},\"rot\":{\"x\":0,\"y\":0,\"z\":0},\"uuid\":\"xxxxxx\"}","{\"cat\":2,\"biz\":3,\"cmd\":1,\"id\":\"101\",\"groupId\":\"100\",\"cst\":1682302519717,\"pos\":{\"x\":0.30000000000000004,\"y\":0,\"z\":0},\"rot\":{\"x\":0,\"y\":0,\"z\":0},\"uuid\":\"xxxxxx\"}"]
```
- client 101, get the client 100'1 message from other worker process:
```
$  node wsc.js 100 101
100 101
connecting to server % ws://127.0.0.1:5050/test?debug=true&AUTH=abc&id=101&name=101
connected to server % ws://127.0.0.1:5050/test?debug=true&AUTH=abc&id=101&name=101
------
[2] ["{\"cat\":2,\"biz\":0,\"cmd\":1,\"id\":\"100\",\"groupId\":\"100\",\"cst\":1682302514567,\"pos\":{\"x\":0.30000000000000004,\"y\":0,\"z\":0},\"rot\":{\"x\":0,\"y\":0,\"z\":0},\"uuid\":\"xxxxxx\"}","{\"cat\":2,\"biz\":3,\"cmd\":1,\"id\":\"100\",\"groupId\":\"100\",\"cst\":1682302514567,\"pos\":{\"x\":0.30000000000000004,\"y\":0,\"z\":0},\"rot\":{\"x\":0,\"y\":0,\"z\":0},\"uuid\":\"xxxxxx\"}"]
------
[2] ["{\"cat\":2,\"biz\":0,\"cmd\":1,\"id\":\"100\",\"groupId\":\"100\",\"cst\":1682302517579,\"pos\":{\"x\":0.4,\"y\":0,\"z\":0},\"rot\":{\"x\":0,\"y\":0,\"z\":0},\"uuid\":\"xxxxxx\"}","{\"cat\":2,\"biz\":3,\"cmd\":1,\"id\":\"100\",\"groupId\":\"100\",\"cst\":1682302517579,\"pos\":{\"x\":0.4,\"y\":0,\"z\":0},\"rot\":{\"x\":0,\"y\":0,\"z\":0},\"uuid\":\"xxxxxx\"}"]
------
[2] ["{\"cat\":2,\"biz\":0,\"cmd\":1,\"id\":\"100\",\"groupId\":\"100\",\"cst\":1682302520587,\"pos\":{\"x\":0.5,\"y\":0,\"z\":0},\"rot\":{\"x\":0,\"y\":0,\"z\":0},\"uuid\":\"xxxxxx\"}","{\"cat\":2,\"biz\":3,\"cmd\":1,\"id\":\"100\",\"groupId\":\"100\",\"cst\":1682302520587,\"pos\":{\"x\":0.5,\"y\":0,\"z\":0},\"rot\":{\"x\":0,\"y\":0,\"z\":0},\"uuid\":\"xxxxxx\"}"]```
```
