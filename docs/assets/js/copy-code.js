document.addEventListener('DOMContentLoaded', () => {
    // Add copy buttons to all code blocks
    document.querySelectorAll('div.highlight').forEach(highlightDiv => {
        // Check if button already exists
        if (highlightDiv.querySelector('.copy-code-button')) {
            return;
        }
        
        // Create the copy button
        const button = document.createElement('button');
        button.className = 'copy-code-button';
        button.type = 'button';
        button.setAttribute('aria-label', 'Copy code to clipboard');
        
        // Add button to highlight div
        highlightDiv.appendChild(button);
        
        // Add click event
        button.addEventListener('click', async () => {
            const code = highlightDiv.querySelector('code');
            const text = code.innerText;
            
            // Copy to clipboard
            try {
                await navigator.clipboard.writeText(text);
                
                // Visual feedback
                button.classList.add('copied');
                
                // Reset button after 2 seconds
                setTimeout(() => {
                    button.classList.remove('copied');
                }, 2000);
            } catch (err) {
                console.error('Failed to copy code:', err);
                button.style.backgroundColor = 'rgba(255, 0, 0, 0.1)';
                
                // Reset button after 2 seconds
                setTimeout(() => {
                    button.style.backgroundColor = '';
                }, 2000);
            }
        });
    });
}); 