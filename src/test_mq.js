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
