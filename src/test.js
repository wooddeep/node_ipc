const cluster = require("cluster")
const Router = require('koa-router')
const Koa = require('koa')
const bodyParser = require('koa-bodyparser')
const backend = require("../index.js")
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

// router.get('/read', async (ctx) => {
//     backend.testShmRead()
//     ctx.body = 'testShmRead response'
// });
//
// //  curl -H "Content-Type:application/json" -X POST http://127.0.0.1:5050/write -d '{"key": "val"}'
// router.post('/write', async (ctx) => {
//     await backend.testShmWrite(JSON.stringify(ctx.request.body))
//     ctx.body = 'testShmWrite response'
// })

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

async function main() {
    if (cluster.isMaster) { // main process
        cluster.schedulingPolicy = cluster.SCHED_RR;

        await backend.masterInit(child_proc_num);
        backend.listen(async (data) => {
            console.log(`##[master:${process.pid}] msg from other process | process id: ${process.pid}; data.length = ${data.length}, data = ${data}, time = ${new Date()}`)
        });

        setTimeout(async () => { // wait for worker's named pipe create
            await backend.establish();
            setInterval(() => {
                backend.publish(1, `msg form worker[${process.pid}]`); // send message to 1st worker
            }, 3000);
        }, 3000);

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

        await backend.workerInit(child_proc_num, Number.parseInt(process.env["WORKER_INDEX"]));
        backend.listen(async (data) => {
            console.log(`##[worker:${process.pid}] msg from other process | process id: ${process.pid}; data.length = ${data.length}, data = ${data}, time = ${new Date()}`)
        });

        await backend.establish();
        setInterval(() => {
            backend.publish(0, `msg form worker[${process.pid}]`);
        }, 3000);

        const emitter = new events.EventEmitter();

        backend.regNodeFunc(async (data) => {
            if (data.length > 2) {
                //console.log(`## process id: ${process.pid}; data.length = ${data.length}, data = ${data}, time = ${new Date()}`)
                emitter.emit("peer", data)
            }
        });

        process.WORKER_INDEX = process.env["WORKER_INDEX"]
        console.log("WORKER_INDEX", process.env["WORKER_INDEX"])

        const websockd = new wsd(emitter);

        websockd.start(emitter);
    }
}

main().then(() => 0);
