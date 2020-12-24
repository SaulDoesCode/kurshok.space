{
    let o = new IntersectionObserver((a, s) => a.forEach(e =>{e.isIntersecting && (e.target.src = e.target.dataset.l, s.unobserve(e.target))}));var lazySimon=()=> [...document.querySelectorAll("img")].forEach(e => {e.dataset.l = e.src; e.src = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mN89xQAAscB1RY/sEQAAAAASUVORK5CYII="; o.observe(e)});
    lazySimon()
}