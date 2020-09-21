import app from '/js/site.min.js'
import route from '/js/router.min.js'
const d = app.d, df = d.domfn
const {div, h4, section, span, header} = df

const contentDisplay = df.section({class: 'posts'})

route('posts', contentDisplay)
if (location.hash == '' || location.hash == '#') location.hash = 'posts'

app.view = {
    page: 0,
}

const publicPost = (w) => div({
    $: contentDisplay,
    class: 'post',
    attr: {pid: w.id}
},
    header(
        div(
            h4(w.title),
            div({class: 'author-name'}, `By ${w.author_name}`)
        ),
        div(
            div({class: 'posted'}, new Date(w.posted * 1000).toLocaleString()),
            div({class: 'tags'},
                w.tags.map(t => span({class: 'tag'}, t))
            )
        )
    )
);

app.fetchPostContent = async id => {
    const res = await fetch('/post-content/' + id)
    const data = await res.json()
    if (data.ok) return data.data
    throw new Error(data.status)
}

app.writQuery({with_content: false}).then(writs => {
    if(!d.isArr(writs)) return console.error(writs)
    writs.forEach(w => publicPost(w))
})