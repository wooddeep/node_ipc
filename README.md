# node_ipc
ipc for nodejs base on share memory and semaphore! [jump to github](https://github.com/wooddeep/node_ipc)

# description
  The default IPC mechanism of Node.js does not support direct communication between worker processes, only communication between the master process and worker processes. 
  Communication between worker processes requires intermediate transfer through the master process. The purpose of developing this project is to implement IPC communication 
  between worker processes in Node.js cluster mode through napi-rs. IPC between processes is achieved through shared memory and semaphores. Currently, there are no 
  open-source Node.js plugins available on the Internet, so I'm implementing one myself.  

  Support platform: window, linux 

# usage 
1. semaphore test:
  - 1.1 source code
```javascript
// test_sem.js
let backend = require("@wooddeep/node_ipc")
const bodyParser = require('koa-bodyparser')
const Router = require('koa-router')
const Koa = require('koa')

const app = new Koa()
let router = new Router()
app.use(bodyParser())

router.get('/create', async (ctx) => {
  await backend.semCreate("test")
  ctx.body = 'semCreate response'
});

router.get('/require', async (ctx) => {
  await backend.semRequire("test")
  ctx.body = 'semRequire response'
});

router.get('/release', async (ctx) => {
  await backend.semRelease("test")
  ctx.body = 'semRelease response'
});

app.use(router.routes()).use(router.allowedMethods())


process.on("SIGINT", () => {
  backend.processExit()
  process.exit()
});

process.on("beforeExit", (code) => {
  console.log("## pre exit in node...")
  backend.processExit()
})

async function main() {

  let port = Number.parseInt(process.argv[2]);
  port = port == undefined ? 3000 : port;

  app.listen(port, () => {
    console.log(`start server at localhost:${port}`);
  })

}

main().then(() => 0);

```
  - 1.2 test script:
```bash
# start the first server
node test_sem.js 3000

# start the second server
node test_sem.js 3001

# create semaphore and require sem
curl "http://127.0.0.1:3000/create"
curl "http://127.0.0.1:3000/require"

# release sem
curl "http://127.0.0.1:3001/release"
```

2. share memory test:
- 2.1 source code
```javascript
// test_shm.js
let backend = require("@wooddeep/node_ipc")
const bodyParser = require('koa-bodyparser')
const backend = require("../index.js")
const Router = require('koa-router')
const Koa = require('koa')

const app = new Koa()
let router = new Router()
app.use(bodyParser())

// curl "http://127.0.0.1:3000/write?shm_name=rustMapping&content=helloworld"
router.get('/write', async (ctx) => {
  let shm_name = ctx.request.query["shm_name"];
  shm_name = shm_name == undefined ? "RustMapping" : shm_name;

  let content = ctx.request.query["content"];
  content = content == undefined ? "hello world!" : content;

  backend.shmOpen(shm_name, 1024);

  backend.shmWriteStr(shm_name, 0, content)
  ctx.body = '{"code": 0, "msg":"write ok!"}'
});

// curl -v http://127.0.0.1:3001/write?shm_name=rustMapping
router.get('/read', async (ctx) => {
  let shm_name = ctx.request.query["shm_name"];
  shm_name = shm_name == undefined ? "RustMapping" : shm_name;
  backend.shmOpen(shm_name, 1024);
  let data = backend.shmReadStr(shm_name, 0, 1024);

  ctx.body = data
});

app.use(router.routes()).use(router.allowedMethods())

process.on("SIGINT", () => {
  backend.processExit()
  process.exit()
});

process.on("beforeExit", (code) => {
  console.log("## pre exit in node...")
  backend.processExit()
})

async function main() {

  let port = Number.parseInt(process.argv[2]);
  port = port == undefined ? 3000 : port;

  app.listen(port, () => {
    console.log(`start server at localhost:${port}`);
  })
}

main().then(() => 0);


```
- 2.2 test script:
```bash
# start the first server
node test_shm.js 3000

# start the second server
node test_shm.js 3001

# write string to shm
curl "http://127.0.0.1:3000/write?shm_name=rustMapping&content=helloworld"

# read string from shm
curl -v "http://127.0.0.1:3001/write?shm_name=rustMapping"
```

3. message queue test:
- 3.1 source code
```javascript
// test_mq.js
let backend = require("@wooddeep/node_ipc")
const cluster = require("cluster")

const backend = require("../index.js")
const events = require("events")

const child_proc_num = 3; // /*os.cpus().length*/

process.on("SIGINT", () => {
  backend.processExit()
  process.exit()
});

process.on("beforeExit", (code) => {
  console.log("## pre exit in node...")
  backend.processExit()
})

async function main() {
  if (cluster.isMaster) { // main process
    cluster.schedulingPolicy = cluster.SCHED_RR;

    let mq_index = await backend.mqCreate("0");
    console.log(`mq create, index: ${mq_index}`);

    backend.mqListen(async (data) => {
      console.log(`##[master:${process.pid}] msg from other process; data.length = ${data.length}, data = ${data}, time = ${new Date()}`)
    }, mq_index);

    let child_map = new Map()
    for (var i = 0, n = child_proc_num; i < n; i += 1) {
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

    let sender_index = await backend.mqEstablish("0");
    setInterval(() => {
      let data = `msg form worker[${process.pid}]`
      backend.mqPublish(sender_index, data);
    }, 2000);
    //const emitter = new events.EventEmitter();
  }
}

main().then(() => 0);
```

- 3.2 test script:
```bash
# start the server cluster
node test_mq.js

# observing the main process receiving data from three child processes in in console ouput:
##[master:14216] msg from other process; data.length = 1024, data = msg form worker[14226], time = Wed Jun 07 2023 15:34:56 GMT+0800 (China Standard Time)
##[master:14216] msg from other process; data.length = 1024, data = msg form worker[14227], time = Wed Jun 07 2023 15:34:56 GMT+0800 (China Standard Time)
##[master:14216] msg from other process; data.length = 1024, data = msg form worker[14229], time = Wed Jun 07 2023 15:34:56 GMT+0800 (China Standard Time)
```

