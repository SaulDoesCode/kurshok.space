import app from '/js/site.min.js'
const d = app.d, df = d.domfn

const authLauncher = d.query('.auth-launcher')

const {
  authView,
  usernameInput,
  passwordInput,
  authBtn
} = d.h `
<section class="auth-view" ref="authView">
  <div>
    <div class="auth-form-field">
      <label for="auth-username">Username</label>
      <input type="text" name="username" id="auth-username" ref="usernameInput">
      <label for="auth-pwd">Password</label>
      <input type="password" name="password" id="auth-pwd" ref="passwordInput">
    </div>
    <button class="submit" ref="authBtn">authenticate</button>
  </div>
</section>`.collect()

app.authView = authView

app.authViewToggle = (state = !df.hasClass(authView, 'open')) => {
  df.class(authView,'open', state)
  df.class(authLauncher,'active', state)
  if (state) {
    d.render(authView)
    clickOutHandler.on()
  } else {
    df.remove(authView)
  }
}

const clickOutHandler = d.on.pointerdown(document.body, e => {
  if (
    e.target != authView &&
    !authView.contains(e.target) &&
    df.hasClass(authView, 'open') &&
    e.target != authLauncher
  ) {
    e.preventDefault()
    app.authViewToggle(false)
    clickOutHandler.off()
  }
}).off()

d.on.pointerdown(authLauncher, app.clickAuthLauncher = e => {
  e.preventDefault()
  app.authViewToggle()
})

app.authenticate = async (
  username = usernameInput.value.trim(),
  password = passwordInput.value.trim()
) => {
  if (username == '') throw new Error('username is invalid')
  if (password == '') throw new Error('password is invalid')
  console.log('attempting authentication')
  const res = await app.jsonPost('/auth', { username, password })
  const data = await res.json()
  console.log(data)
  if (data.ok) location.reload()
}

const authClickHandle = d.once.click(authBtn, async e => {
  try {
    await app.authenticate()
  } catch(e) {
    console.error(e)
    authClickHandle.on()
  }
})

app.authViewToggle()