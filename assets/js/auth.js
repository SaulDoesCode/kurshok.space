{
  const authform = document.querySelector('.auth')
  const usernameInput = authform.querySelector('input[name="username"]')
  const passwordInput = authform.querySelector('input[name="password"]')

  window.onclick = e => {
    const isOpen = authform.classList.contains('open')
    if (e.target == authform || authform.contains(e.target)) {
      if (!isOpen) {
        authform.classList.add('open')
        e.preventDefault()
      }
    } else if (isOpen) {
      authform.classList.remove('open')
    }
  }

  app.auth = (
    username = usernameInput.value.trim(),
    password = passwordInput.value.trim()
  ) => {
    app.jsonPost('/auth', { username, password })
    .then(res => res.json())
    .then(data => {
      console.log(data)
      if (data.ok) location.reload()
    })
  }

  app.authContingent = () => authform.classList.contains('open') && app.auth()
}
