// Simple check for ethers.js load status
console.log('🔍 Checking ethers.js load status...');

// Check if ethers is loaded
function waitForEthers(callback, maxAttempts = 50) {
    let attempts = 0;
    
    function check() {
        attempts++;
        if (typeof window.ethers !== 'undefined') {
            console.log('✅ ethers.js loaded!', window.ethers.version);
            callback();
        } else if (attempts < maxAttempts) {
            console.log(`⏳ Waiting for ethers.js to load... (${attempts}/${maxAttempts})`);
            setTimeout(check, 100);
        } else {
            console.error('❌ ethers.js load timeout');
            alert('Failed to load ethers.js. Please check your network connection or refresh the page and try again.');
        }
    }
    
    check();
}

// Check ethers after DOM is ready
document.addEventListener('DOMContentLoaded', function() {
    waitForEthers(function() {
        console.log('🎉 ethers.js ready!');
    });
}); 