const dropContainer = document.getElementById("dropcontainer");
const fileInput = document.getElementById("fileInput");
const dropTitle = document.getElementById("dropfilename");
const uploadButton = document.getElementById("uploadButton");
const confirmDialog = document.getElementById("confirmDialog");
const uploadPreview = document.getElementById("uploadPreview");
const fileStats = document.getElementById("fileStats");

const uploadFilename = document.getElementById("filenameUploading");
const uploadStats = document.getElementById("uploadStats");
const uploadProgress = document.getElementById("uploadProgress");
const uploadDialog = document.getElementById("uploadDialog");

const chunkSize = 2 * 1024 * 1024; // 2MB chunks
const ACK_EVERY = 4;
let fileId = null;
let ws = null;
let awaitingAck = false;

const setDropTitle = () => {
    dropTitle.innerText = fileInput.files[0].name;
    fileStats.innerHTML = `Size: ${bytesToSize(fileInput.files[0].size)}`;

    // update preview if the file is an image
    const file = fileInput.files[0];
    if (file.type.startsWith("image/")) {
        const reader = new FileReader();
        reader.onload = (e) => {
            uploadPreview.src = e.target.result;
            uploadPreview.classList.remove("d-none");
        };
        reader.readAsDataURL(file);
    } else {
        uploadPreview.classList.add("d-none");
    }
};

const checkFile = () => {
    if (fileInput.files.length > 0) {
        dropContainer.classList.add("d-none");
        confirmDialog.classList.remove("d-none");
    } else {
        dropContainer.classList.remove("d-none");
        confirmDialog.classList.add("d-none");
    }
};

dropContainer.addEventListener("dragover", (e) => e.preventDefault(), false);
dropContainer.addEventListener("dragenter", () => dropContainer.classList.add("drag-active"));
dropContainer.addEventListener("dragleave", () => dropContainer.classList.remove("drag-active"));

dropContainer.addEventListener("drop", (e) => {
    e.preventDefault();
    dropContainer.classList.remove("drag-active");
    fileInput.files = e.dataTransfer.files;
    setDropTitle();
    checkFile();
});

fileInput.addEventListener("change", () => {
    setDropTitle();
    checkFile();
});

window.addEventListener("paste", (e) => {
    const items = e.clipboardData?.items;
    if (!items) return;

    for (const item of items) {
        if (item.kind === "file") {
            const file = item.getAsFile();
            if (file) {
                let filename = file.name;

                // rename generic filenames
                if (!filename || filename === "image.png" || filename.startsWith("blob")) {
                    const ext = file.type.split("/")[1] || "bin";
                    filename = `pasted_${Date.now()}.${ext}`;
                }

                const renamedFile = new File([file], filename, { type: file.type });
                const dataTransfer = new DataTransfer();
                dataTransfer.items.add(renamedFile);
                fileInput.files = dataTransfer.files;

                setDropTitle();
                checkFile();
                break;
            }
        }
    }
});

function cancelFile() {
    if (ws) ws.close();
    fileInput.value = "";
    awaitingAck = false; // Reset global state
    checkFile();
    uploadDialog.classList.add("d-none");
}

function padZero(num) {
    return num.toString().padStart(2, "0");
}

const updateStats = (percentage, uploaded, total, speed, remaining) => {
    uploadProgress.style.width = `${percentage}%`;
    uploadStats.innerHTML = `Uploading... ${percentage.toFixed(2)}% <br/>
        ${bytesToSize(uploaded)} / ${bytesToSize(total)} <br/>
        <i class="fas fa-arrow-up"></i> ${bytesToSize(speed)}/s <br/>
        <i class="fas fa-clock"></i> ${remaining}`;
};

function upload() {
    if (!fileInput.files.length) return;
    const file = fileInput.files[0];
    uploadFilename.innerText = file.name;
    uploadDialog.classList.remove("d-none");
    confirmDialog.classList.add("d-none");
    updateStats(0, 0, file.size, 0, "Calculating...");

    const url = new URL(window.location.href);
    const wsProtocol = url.protocol === "https:" ? "wss:" : "ws:";
    ws = new WebSocket(`${wsProtocol}//${url.host}/upload`);
    ws.binaryType = "arraybuffer";

    let hasStarted = false;

    ws.onmessage = (event) => {
        const data = new Uint8Array(event.data);
        if (data[0] === 0 && !hasStarted) {
            // first packet confirms the upload
            hasStarted = true;
            awaitingAck = false; // Reset ack flag
            sendChunks(file);
            console.log("Upload confirmed by server");
        } else if (data[0] === 1 && hasStarted) {
            // acknowledgment packet - server is ready for next chunk
            awaitingAck = false;
        } else if (hasStarted && !awaitingAck) {
            // final packet contains the file ID
            ws.close();
            fileId = new TextDecoder().decode(data);
            window.location.href = `/f/${fileId}`;
        } else {
            // server rejected the upload
            ws.close();
            cancelFile();
        }
    };

    ws.onopen = () => {
        // send file info (size + chunk count + name)
        const fileSize = file.size;
        const fileName = file.name;
        const chunkCount = Math.ceil(fileSize / chunkSize);

        let fileNameData = new TextEncoder().encode(fileName);
        const fileInfo = new Uint8Array(16 + fileNameData.byteLength);
        new DataView(fileInfo.buffer).setBigUint64(0, BigInt(fileSize), true);
        new DataView(fileInfo.buffer).setBigUint64(8, BigInt(chunkCount), true);
        fileInfo.set(fileNameData, 16);

        ws.send(fileInfo);
    };

    ws.onerror = (error) => console.error("WebSocket error:", error);

    // Add connection timeout
    setTimeout(() => {
        if (!hasStarted && ws.readyState === WebSocket.OPEN) {
            console.error("Upload timeout - server didn't respond");
            ws.close();
            cancelFile();
        }
    }, 30000); // 30 second timeout
}

async function sendChunks(file) {
    const fileSize = file.size;
    const chunkCount = Math.ceil(fileSize / chunkSize);

    let chunkIndex = 0;
    let uploaded = 0;
    let actualSent = 0; // Track actual bytes sent over network
    let startTime = Date.now();
    let lastSpeedCheck = Date.now();
    let speed = 0;
    let remaining = 0;

    const statsUpdateInterval = setInterval(() => {
        // Calculate speed based on actual sent data
        const now = Date.now();
        const timeDelta = (now - startTime) / 1000; // seconds
        if (timeDelta > 0) {
            speed = actualSent / timeDelta;
        }

        const remainingBytes = fileSize - actualSent;
        const remainingTime = speed > 0 ? (remainingBytes / speed) * 1000 : 0;

        const remainingSeconds = padZero(Math.floor(remainingTime / 1000) % 60);
        const remainingMinutes = padZero(Math.floor(remainingTime / 60000) % 60);
        const remainingHours = padZero(Math.floor(remainingTime / 3600000));

        updateStats(
            (actualSent / fileSize) * 100,
            actualSent,
            fileSize,
            speed,
            `${remainingHours}:${remainingMinutes}:${remainingSeconds} remaining`
        );
    }, 500);

    const sendNextChunk = () => {
        if (chunkIndex >= chunkCount) {
            clearInterval(statsUpdateInterval);
            updateStats(100, fileSize, fileSize, 0, "Saving your file...");
            return;
        }

        if (awaitingAck) {
            setTimeout(sendNextChunk, 10);
            return;
        }

        const start = chunkIndex * chunkSize;
        const end = Math.min(start + chunkSize, fileSize);
        const chunk = file.slice(start, end);

        const reader = new FileReader();
        reader.onload = () => {
            if (ws.readyState === WebSocket.OPEN) {
                const chunkData = reader.result;

                // Send and track when actually sent
                ws.send(chunkData);

                // Update counters
                uploaded += chunk.size; // File reading progress
                actualSent += chunkData.byteLength; // Actual network transmission
                chunkIndex++;

                // Set awaiting acknowledgment flag
                awaitingAck = ((chunkIndex - 1) % ACK_EVERY === 0);

                setTimeout(sendNextChunk, 5);
            }
        };

        reader.onerror = () => {
            console.error("Error reading file chunk");
            ws.close();
            cancelFile();
        };

        reader.readAsArrayBuffer(chunk);
    };

    sendNextChunk();
}