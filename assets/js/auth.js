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

app.authViewToggle = app.setupToggleSituation(authLauncher, app.authView = authView)

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

app.authViewToggle.toggleView()