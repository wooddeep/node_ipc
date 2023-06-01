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

const child_proc_num = 2; // /*os.cpus().length*/

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

        let mq_index = await backend.mqCreate("0");
        console.log(`mq create, index: ${mq_index}`);

        backend.listen(async (data) => {
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

        await backend.workerInit(child_proc_num, Number.parseInt(process.env["WORKER_INDEX"]));

        let sender_index = await backend.establish("0");
        setInterval(() => {
            let data = `msg form worker[${process.pid}]`
            console.log(`worker:[${process.pid}], sender index: ${sender_index}, send length: ${data.length}`);
            backend.publish(sender_index, data);
        }, 2000);


        const emitter = new events.EventEmitter();

        backend.regNodeFunc(async (data) => {
            if (data.length > 2) {
                //console.log(`## process id: ${process.pid}; data.length = ${data.length}, data = ${data}, time = ${new Date()}`)
                emitter.emit("peer", data)
            }
        });

        process.WORKER_INDEX = process.env["WORKER_INDEX"]

        const websockd = new wsd(emitter);

        websockd.start(emitter);
    }
}

main().then(() => 0);
