import { Editor } from './Editor';

// Initialize the editor when the DOM is ready
document.addEventListener('DOMContentLoaded', () => {
  const statusIndicator = document.getElementById('status-indicator')!;
  const statusText = document.getElementById('status-text')!;
  const rootHashDisplay = document.getElementById('root-hash')!;
  const charCountDisplay = document.getElementById('char-count')!;
  const proofCountDisplay = document.getElementById('proof-count')!;
  const opCountDisplay = document.getElementById('op-count')!;
  const notificationElement = document.getElementById('notification')!;

  const editor = new Editor(
    statusIndicator,
    statusText,
    rootHashDisplay,
    charCountDisplay,
    proofCountDisplay,
    opCountDisplay,
    notificationElement
  );

  editor.init();

  // Clean up on page unload
  window.addEventListener('beforeunload', () => {
    editor.destroy();
  });
});
