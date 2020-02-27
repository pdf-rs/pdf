wasm_bindgen("pkg/pdf_view_bg.wasm").catch(console.error)
.then(show_logo);
//display("Drop a PDF here");


function show_logo() {
    fetch("logo.pdf")
    .then(r => r.arrayBuffer())
    .then(buf => show_data(new Uint8Array(buf)));
}

function set_scroll_factors() {}

function drop_handler(e) {
    e.stopPropagation();
    e.preventDefault();
    show(e.dataTransfer.files[0]);
}
function dragover_handler(e) {
    e.stopPropagation();
    e.preventDefault();
}

function display(msg) {
    delete document.getElementById("drop").style.display;
    document.getElementById("msg").innerText = msg;
}

let view;
function init_view(data) {
    let canvas = document.getElementById("canvas");
    view = wasm_bindgen.show(canvas, data);

    let requested = false;
    function animation_frame(time) {
        requested = false;
        view.animation_frame(time);
    }
    function check(request_redraw) {
        if (request_redraw && !requested) {
            window.requestAnimationFrame(animation_frame);
            requested = true;
        }
    }

    canvas.addEventListener("keydown", e => check(view.key_down(e)));
    canvas.addEventListener("keyup", e => check(view.key_up(e)));
    canvas.addEventListener("mousemove", e => check(view.mouse_move(e)));
    canvas.addEventListener("mouseup", e => check(view.mouse_up(e)));
    canvas.addEventListener("mousedown", e => check(view.mouse_move(e)));
    window.addEventListener("resize", e => check(view.resize(e)));
    view.render();
}

function show_data(data) {
    try {
        init_view(data);
        document.getElementById("drop").style.display = "none";
    } catch (e) {
        display("oops. try another one.");
        display(e);
    }
}

function show(file) {
    let reader = new FileReader();
    reader.onload = function() {
        let data = new Uint8Array(reader.result);
        show_data(data);

    };
    reader.readAsArrayBuffer(file);
}

document.addEventListener("drop", drop_handler, false);
document.addEventListener("dragover", dragover_handler, false);

