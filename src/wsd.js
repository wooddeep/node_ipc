const WebSocket = require('ws')
const {initProcInfo, runServer, testShmWrite, testShmRead, printThreadId, testShmWriteThread, callback} = require("../index");
const backend = require("../index.js");

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