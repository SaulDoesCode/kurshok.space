import app from '/js/site.min.js'
const d = app.d, df = d.domfn

const profileLauncher = d.query('.profile-launcher')

const {
    profileView,
    logoutBtn
} = d.h/* html */`
<section class="profile-view" ref="profileView">
    <h4>Profile of ${app.user.username}</h4>
    <button class="logout-btn" ref="logoutBtn">Log out</button>
</section>`.collect()

app.profileViewToggle = app.setupToggleSituation(
    profileLauncher,
    app.profileView = profileView,
    'body',
    {background: true}
)

d.once.click(logoutBtn, async e => {
    const res = await (await fetch('/auth', {method: 'DELETE'})).json()
    df.remove(logoutBtn)
    profileView.append(df.div(res.status))
    setTimeout(() => window.location.reload(), 1000)
})