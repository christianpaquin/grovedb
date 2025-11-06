// Message types matching the server
export interface InitialMessage {
  type: 'initial';
  content: Array<[string, string]>;
  root_hash: string;
}

export interface OperationMessage {
  type: 'operation';
  operation: 'insert' | 'delete';
  position: number;
  uuid?: string;
  value?: string;
  root_hash: string;
  proof: string;
}

export interface ErrorMessage {
  type: 'error';
  message: string;
}

export type ServerMessage = InitialMessage | OperationMessage | ErrorMessage;

export interface ClientMessage {
  type: 'insert' | 'delete';
  position: number;
  uuid?: string;  // Client-generated UUID for inserts
  value?: string;
}

export interface DocumentState {
  content: Array<[string, string]>; // [uuid, char] pairs
  rootHash: string;
}

export class DocumentClient {
  private ws: WebSocket | null = null;
  private state: DocumentState = { content: [], rootHash: '' };
  private proofCount = 0;
  private opCount = 0;

  constructor(
    private onStateChange: (state: DocumentState) => void,
    private onConnectionChange: (connected: boolean) => void,
    private onError: (error: string) => void,
    private onStats: (proofCount: number, opCount: number) => void
  ) {}

  async init() {
    // Note: In a production system, this is where you would:
    // 1. Generate or load the user's key pair (e.g., Ed25519)
    // 2. Initialize signature verification for other users' operations
    // For this demo, we trust the server and don't implement actual crypto
    
    console.log('[DocumentClient] Initializing (signatures not implemented in demo)');

    // Connect to WebSocket
    this.connect();
  }

  private connect() {
    const wsUrl = `ws://${window.location.hostname}:3000/ws`;
    this.ws = new WebSocket(wsUrl);

    this.ws.onopen = () => {
      console.log('WebSocket connected');
      this.onConnectionChange(true);
    };

    this.ws.onclose = () => {
      console.log('WebSocket disconnected');
      this.onConnectionChange(false);
      
      // Attempt to reconnect after 3 seconds
      setTimeout(() => this.connect(), 3000);
    };

    this.ws.onerror = (err) => {
      console.error('WebSocket error:', err);
      this.onError('Connection error');
    };

    this.ws.onmessage = (event) => {
      try {
        const msg: ServerMessage = JSON.parse(event.data);
        this.handleServerMessage(msg);
      } catch (err) {
        console.error('Failed to parse server message:', err);
        this.onError('Invalid server message');
      }
    };
  }

  private handleServerMessage(msg: ServerMessage) {
    switch (msg.type) {
      case 'initial':
        this.handleInitial(msg);
        break;
      case 'operation':
        this.handleOperation(msg);
        break;
      case 'error':
        this.onError(msg.message);
        break;
    }
  }

  private handleInitial(msg: InitialMessage) {
    this.state = {
      content: msg.content,
      rootHash: msg.root_hash,
    };
    this.onStateChange(this.state);
  }

  private async handleOperation(msg: OperationMessage) {
    console.log('[DocumentClient] Received operation:', JSON.stringify(msg, null, 2));
    console.log('[DocumentClient] Current state before:', JSON.stringify(this.state.content, null, 2));
    
    // In a production system, verify the operation signature here:
    // const signature = msg.signature; // Would be included in message
    // const userId = msg.userId;       // Would be included in message
    // const isValid = await verifySignature(userId, operation, signature);
    // if (!isValid) {
    //   this.onError('Invalid operation signature!');
    //   return;
    // }
    //
    // For this demo, we trust the server to order and broadcast operations correctly.
    
    this.proofCount++; // Track that server generated a proof (for audit trail)
    this.opCount++;

    // Apply the operation locally
    if (msg.operation === 'insert' && msg.uuid && msg.value) {
      // Check if we already have this UUID (our own optimistic update)
      const existingIndex = this.state.content.findIndex(([uuid, _]) => uuid === msg.uuid);
      
      console.log('[DocumentClient] Looking for UUID:', msg.uuid, 'existingIndex:', existingIndex);
      
      if (existingIndex === -1) {
        // This is from another client, insert it
        console.log('[DocumentClient] Inserting from another client at position', msg.position);
        this.state.content.splice(msg.position, 0, [msg.uuid, msg.value]);
        console.log('[DocumentClient] State after insert:', JSON.stringify(this.state.content, null, 2));
      } else {
        // This is confirmation of our own operation
        console.log('[DocumentClient] Confirmed our own insert at index', existingIndex);
        // No need to do anything, we already have it optimistically
      }
    } else if (msg.operation === 'delete') {
      // For delete, find and remove by position
      // We may have already removed it optimistically
      if (msg.position < this.state.content.length) {
        console.log('[DocumentClient] Deleting at position', msg.position);
        this.state.content.splice(msg.position, 1);
      } else {
        console.log('[DocumentClient] Delete position out of bounds, already removed');
      }
    }

    // Update the root hash
    this.state.rootHash = msg.root_hash;
    console.log('[DocumentClient] Updated root hash to:', this.state.rootHash);

    // Notify listeners
    console.log('[DocumentClient] Calling onStateChange with content length:', this.state.content.length);
    this.onStateChange(this.state);
    this.onStats(this.proofCount, this.opCount);
  }

  // Generate a UUID v4 (random)
  private generateUuid(): string {
    // Simple UUID v4 generation without dependencies
    return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
      const r = Math.random() * 16 | 0;
      const v = c === 'x' ? r : (r & 0x3 | 0x8);
      return v.toString(16);
    });
  }

  // Send an insert operation to the server
  insert(position: number, char: string) {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      this.onError('Not connected to server');
      return;
    }

    // Generate UUID for this character
    const uuid = this.generateUuid();

    // In a production system, sign the operation here:
    // const operation = { type: 'insert', position, uuid, value: char };
    // const signature = await signOperation(userPrivateKey, operation);
    // Then include signature in the message sent to server
    
    // Optimistic update: add to local state immediately
    this.state.content.splice(position, 0, [uuid, char]);
    this.onStateChange(this.state);

    const msg: ClientMessage = {
      type: 'insert',
      position,
      uuid,
      value: char,
      // signature would go here in real system
    };

    console.log('[DocumentClient] Sending insert:', msg);
    this.ws.send(JSON.stringify(msg));
  }

  // Send a delete operation to the server
  delete(position: number) {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      this.onError('Not connected to server');
      return;
    }

    // In a production system, sign the operation here:
    // const operation = { type: 'delete', position };
    // const signature = await signOperation(userPrivateKey, operation);
    // Then include signature in the message sent to server

    // Optimistic update: remove from local state immediately
    if (position < this.state.content.length) {
      this.state.content.splice(position, 1);
      this.onStateChange(this.state);
    }

    const msg: ClientMessage = {
      type: 'delete',
      position,
      // signature would go here in real system
    };

    console.log('[DocumentClient] Sending delete:', msg);
    this.ws.send(JSON.stringify(msg));
  }

  getContent(): string {
    return this.state.content.map(([_, char]) => char).join('');
  }

  disconnect() {
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
  }
}
