const WebSocket = require('ws');
var zlib = require('zlib');

//报文
const msg = { "groupId": "0", "cat": 0, "cmd": 0, "ani": 0, "pos": { "x": 0.0, "y": 0.0, "z": 0.0 }, "rot": { "x": 0.0, "y": 0.0, "z": 0.0 }, "id": "1" }

// 输入为Buffer
async function inflate(input) {
    var promise = new Promise((resolve) => {
        zlib.inflate(input, function (err, buf) {
            resolve(buf)
        });
    })

    return promise
}

async function deflate(input) {
    var promise = new Promise((resolve) => {
        zlib.deflate(input, function (err, buf) {
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

    var port = 5050
    if (process.argv.length > 4) {
        port = process.argv[4]
    }

    console.log(groupId, userId)

    //groupId = groupId == undefined ? groupId : "100"
    //userId = userId == undefined ? userId : "100"

    // var cli = new TestClient(`ws://127.0.0.1:6060/gopnv/game/sync?debug=true&AUTH=abc&id=100&name=100`)
    //var cli = new TestClient(`wss://meta-meeting-test.migu.cn/test/game/sync?debug=true&AUTH=abc&id=100&name=100`, groupId, userId)
    var cli = new TestClient(`ws://127.0.0.1:${port}/test?debug=true&AUTH=abc&id=${userId}&name=${userId}`, groupId, userId)

    cli.init();
}

startTest();





