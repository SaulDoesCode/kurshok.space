import app from '/js/site.min.js'
const d = app.d, df = d.domfn
const {div, h4, section, span, header} = df

const contentDisplay = d.query('.content-display')

app.view = {
    page: 0,
}

const publicPost = (w) => div({
    $: contentDisplay,
    class: {
        post: true,
        'with-content': w.content != null
    },
    attr: {pid: w.id}
},
    header(
        h4(w.title),
        div(
            div({class: 'posted'}, new Date(w.posted).toDateString()),
            div({class: 'tags'},
                w.tags.map(t => span({class: 'tag'}, t))
            )
        )
    ),

    w.content != null && section({class: 'content'}, d.html(w.content))
);

app.fetchPostContent = async id => {
    const res = await fetch('/post-content/' + id)
    const data = await res.json()
    if (data.ok) return data.data
    throw new Error(data.status)
}


app.writQuery({
    with_content: false
}).then(writs => {
    if(!d.isArr(writs)) return console.error(writs)
    writs.forEach(w => publicPost(w))
})