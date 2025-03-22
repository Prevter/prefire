function setDarkMode(dark) {
    const html = document.querySelector('html');
    if (dark) {
        html.dataset.bsTheme = 'dark';
        localStorage.setItem('theme', 'dark');
    } else {
        html.dataset.bsTheme = 'light';
        localStorage.setItem('theme', 'light');
    }
}

function toggleDarkMode() {
    const html = document.querySelector('html');
    setDarkMode(html.dataset.bsTheme !== 'dark');
}

function bytesToSize(bytes) {
    const sizes = ['Bytes', 'KB', 'MB', 'GB', 'TB'];
    if (bytes === 0) return '0 Byte';
    const i = parseInt(Math.floor(Math.log(bytes) / Math.log(1024)));
    return Math.round(bytes / Math.pow(1024, i)) + ' ' + sizes[i];
}

(() => {
    // Dark theme toggling
    const html = document.querySelector('html');
    const theme = localStorage.getItem('theme');
    if (theme) html.dataset.bsTheme = theme;
    else {
        const darkModeMediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
        if (darkModeMediaQuery.matches) html.dataset.bsTheme = 'dark';
        else html.dataset.bsTheme = 'light';
    }
})();