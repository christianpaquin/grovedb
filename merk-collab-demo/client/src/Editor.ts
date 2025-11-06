import { DocumentClient, DocumentState } from './DocumentClient';

export class Editor {
  private textarea: HTMLTextAreaElement;
  private client: DocumentClient;
  private isLocalChange = false;
  private lastContent = '';

  constructor(
    private statusIndicator: HTMLElement,
    private statusText: HTMLElement,
    private rootHashDisplay: HTMLElement,
    private charCountDisplay: HTMLElement,
    private proofCountDisplay: HTMLElement,
    private opCountDisplay: HTMLElement,
    private notificationElement: HTMLElement
  ) {
    this.textarea = document.getElementById('editor') as HTMLTextAreaElement;
    
    this.client = new DocumentClient(
      (state) => this.handleStateChange(state),
      (connected) => this.handleConnectionChange(connected),
      (error) => this.showNotification(error, 'error'),
      (proofCount, opCount) => this.updateStats(proofCount, opCount)
    );

    this.setupEventListeners();
  }

  async init() {
    await this.client.init();
  }

  private setupEventListeners() {
    // Handle text input
    this.textarea.addEventListener('input', () => {
      console.log('[Editor] input event fired, isLocalChange:', this.isLocalChange);
      
      if (this.isLocalChange) {
        console.log('[Editor] Ignoring input - local change in progress');
        return;
      }

      const newContent = this.textarea.value;
      console.log('[Editor] newContent:', JSON.stringify(newContent));
      console.log('[Editor] lastContent:', JSON.stringify(this.lastContent));

      // Determine what changed
      const diff = this.getDiff(this.lastContent, newContent);
      
      if (diff) {
        console.log('[Editor] Detected diff:', diff);
        
        // Update lastContent immediately so we see the change
        this.lastContent = newContent;
        
        if (diff.type === 'insert') {
          this.client.insert(diff.position, diff.char);
        } else if (diff.type === 'delete') {
          this.client.delete(diff.position);
        }
      } else {
        console.log('[Editor] No diff detected');
      }
    });

    // Prevent paste for simplicity (would need to handle multiple inserts)
    this.textarea.addEventListener('paste', (e) => {
      e.preventDefault();
      this.showNotification('Paste is disabled in this demo', 'error');
    });
  }

  private getDiff(oldText: string, newText: string): 
    | { type: 'insert'; position: number; char: string }
    | { type: 'delete'; position: number }
    | null {
    
    // Find the first position where they differ
    let i = 0;
    while (i < oldText.length && i < newText.length && oldText[i] === newText[i]) {
      i++;
    }

    if (newText.length > oldText.length) {
      // Insert
      return {
        type: 'insert',
        position: i,
        char: newText[i],
      };
    } else if (newText.length < oldText.length) {
      // Delete
      return {
        type: 'delete',
        position: i,
      };
    }

    return null;
  }

  private handleStateChange(state: DocumentState) {
    console.log('[Editor] handleStateChange called, state:', {
      contentLength: state.content.length,
      rootHash: state.rootHash.substring(0, 16)
    });
    
    // Update textarea with new content
    this.isLocalChange = true;
    console.log('[Editor] Setting isLocalChange = true');
    
    const content = state.content.map(([_, char]) => char).join('');
    console.log('[Editor] New content from state:', JSON.stringify(content));
    
    // Only update if different to avoid cursor jumps
    if (this.textarea.value !== content) {
      console.log('[Editor] Updating textarea value');
      const cursorPos = this.textarea.selectionStart;
      this.textarea.value = content;
      
      // Try to restore cursor position
      this.textarea.setSelectionRange(cursorPos, cursorPos);
    } else {
      console.log('[Editor] Textarea already has correct content');
    }

    this.lastContent = content;
    console.log('[Editor] Updated lastContent to:', JSON.stringify(this.lastContent));
    
    // Update displays
    this.rootHashDisplay.textContent = `Root: ${state.rootHash.substring(0, 16)}...`;
    this.charCountDisplay.textContent = state.content.length.toString();

    this.isLocalChange = false;
    console.log('[Editor] Setting isLocalChange = false');
  }

  private handleConnectionChange(connected: boolean) {
    if (connected) {
      this.statusIndicator.classList.remove('disconnected');
      this.statusText.textContent = 'Connected';
      this.showNotification('Connected to server', 'success');
    } else {
      this.statusIndicator.classList.add('disconnected');
      this.statusText.textContent = 'Disconnected - Reconnecting...';
      this.showNotification('Disconnected from server', 'error');
    }
  }

  private updateStats(proofCount: number, opCount: number) {
    this.proofCountDisplay.textContent = proofCount.toString();
    this.opCountDisplay.textContent = opCount.toString();
  }

  private showNotification(message: string, type: 'success' | 'error') {
    this.notificationElement.textContent = message;
    this.notificationElement.className = `notification ${type} show`;

    setTimeout(() => {
      this.notificationElement.classList.remove('show');
    }, 3000);
  }

  destroy() {
    this.client.disconnect();
  }
}
