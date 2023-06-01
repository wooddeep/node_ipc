const bodyParser = require('koa-bodyparser')
const backend = require("../index.js")
const Router = require('koa-router')
const Koa = require('koa')

const app = new Koa()
let router = new Router()
app.use(bodyParser())

router.post('/write', async (ctx) => {
    var data = ctx.request.body;
    backend.shmWriteStr("test", 0, "hello world!")
    ctx.body = 'semaRequire response'
});

router.get('/read', async (ctx) => {
    let data = backend.shmReadStr("test", 0, 1024);
    ctx.body = data.trim()
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

    if (process.argv[3] == "shm_open") {
        await backend.shmOpen("RustMapping", 1024);
    } else {
        await backend.shmCreate("RustMapping", 1024);
    }

    let port = Number.parseInt(process.argv[2]);
    port = port == undefined ? 3000 : port;

    app.listen(port, () => {
        console.log(`start server at localhost:${port}`);
    })

}

main().then(() => 0);
