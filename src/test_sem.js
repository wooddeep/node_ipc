const bodyParser = require('koa-bodyparser')
const backend = require("../index.js")
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
