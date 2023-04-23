#include <windows.h>
#include <stdio.h>

int send_main() {
   HANDLE hMQ = CreateMessageQueue("myqueue"); // 创建消息队列
    if (hMQ == NULL) {
        printf("Failed to create message queue: %d\n", GetLastError());
        return 1;
    }
    printf("Message queue created.\n");

    char message[1024] = "Hello from sender!"; // 待发送的消息内容
    if (SendMessage(hMQ, message, sizeof(message)) == FALSE) { // 发送消息
        printf("Failed to send message: %d\n", GetLastError());
        return 1;
    }
    printf("Message sent.\n");

    CloseHandle(hMQ); // 关闭消息队列句柄
    return 0;
}

int recv_main() {
    HANDLE hMQ = OpenMessageQueue("myqueue"); // 打开消息队列
    if (hMQ == NULL) {
        printf("Failed to open message queue: %d\n", GetLastError());
        return 1;
    }
    printf("Message queue opened.\n");

    char message[1024]; // 用于存放接收到的消息
    DWORD bytesRead = 0; // 实际读取到的字节数
    if (ReceiveMessage(hMQ, message, sizeof(message), &bytesRead) == FALSE) { // 接收消息
        printf("Failed to receive message: %d\n", GetLastError());
        return 1;
    }
    printf("Received message: %s\n", message);

    CloseHandle(hMQ); // 关闭消息队列句柄
    return 0;
}
