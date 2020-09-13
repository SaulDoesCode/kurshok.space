import app from '/js/site.min.js'
const d = app.d, df = d.domfn

const wwLauncher = d.query('.ww-launcher')

const {
    wwView,
    titleInput,
    writingPad,
    tagInput,
    pushWritBtn,
    deleteWritBtn,
    writSelector,
    writList
} = d.h `
<article class="writ-writer-view" ref="wwView">
    <section class="writer">
        <div>
            <input type="text" name="post-title" title="writ title" id="title-input" placeholder="writ title" autocomplete="off" ref="titleInput">
        </div>
        <div>
            <textarea class="writing-pad" ref="writingPad" title="writ content" spellcheck="true" wrap="off" minlength="10" required placeholder="content of your writ (it can be markdown)"></textarea>
        </div>
        <div>
            <input type="text" name="tags" title="tag input" id="tag-input" placeholder="comma,separated,tags" autocomplete="off" ref="tagInput">
        </div>
        <section class="ribbon">
            <button class="submit" ref="pushWritBtn">Push</button>
            <button class="submit" ref="deleteWritBtn">Delete</button>
        </section>
    </section>
    <aside class="writ-selector" ref="writSelector">
        <header>Writs</header>
        <section class="writ-list" ref="writList"></section>
    </aside>
</article>`.collect()

writingPad.value = ''

;(app.wwTS = app.setupToggleSituation(wwLauncher, wwView)).toggleView()


const writListEntry = (title, id) => d('div', {
    class: 'wl-entry',
    $: writList,
    attr: {wid: id}
},
    d('span', title)
)

app.user.writs = {}
app.ww = {}

app.editableWritQuery({
    author_name: app.user.username,
}).then(async writs => {
    console.log(writs)
    writs.forEach(w => {
        app.user.writs[w.id] = w
        writListEntry(w.title, w.id)
    })

    d.on.click(writList, e => {
        let wid = e.target.getAttribute('wid') || e.target.parentElement.getAttribute('wid')
        if (wid != null) {
            const writ = app.user.writs[wid]
            if (app.ww.selectedWLE) {
                app.ww.selectedWLE.classList.remove('selected')
            }
            app.ww.selectedWLE = d.query(`[wid="${wid}"]`)
            app.ww.selectedWLE.classList.add('selected')

            titleInput.value = writ.title
            writingPad.value = writ.content
            console.log('got one:' , writ)
        }
    })
})