# node_ipc
ipc for nodejs base on share memory and semaphore!

# description
  The default IPC mechanism of Node.js does not support direct communication between worker processes, only communication between the master process and worker processes. Communication between worker processes requires intermediate transfer through the master process. The purpose of developing this project is to implement IPC communication between worker processes in Node.js cluster mode through napi-rs. IPC between processes is achieved through shared memory and semaphores. Currently, there are no open-source Node.js plugins available on the Internet, so I'm implementing one myself.  

  Currently, only direct communication between Node.js worker processes under Windows has been implemented. Subsequently, direct communication between worker processes under Linux and macOS will be gradually implemented.  
