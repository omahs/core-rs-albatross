import * as Comlink from 'comlink';
import { Address, Transaction } from './main-wasm/index.js';
import { clientFactory } from '../client-proxy.mjs';
import { setupMainThreadTransferHandlers } from '../transfer-handlers.mjs';

setupMainThreadTransferHandlers(Comlink, {
    Address,
    Transaction,
});

const Client = clientFactory(
    () => new Worker(new URL('./worker.js', import.meta.url)),
    worker => Comlink.wrap(worker),
);

export * from './main-wasm/index.js';
export { Client };
