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
