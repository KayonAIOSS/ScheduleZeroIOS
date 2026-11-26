import SwiftUI

/// "Who's using this phone?" Only the three accounts exist, so it's a choice, not a text box.
struct PersonPickerView: View {
    let onChosen: (Person) -> Void
    @Environment(\.palette) var p

    var body: some View {
        VStack(spacing: 0) {
            Spacer()
            Text("Who's this?").font(.szHeadline).foregroundStyle(p.onBackground)
            Text("Pick your dot. Your schedule syncs to every device you sign in on.")
                .font(.szBody).foregroundStyle(p.onSurfaceVariant)
                .multilineTextAlignment(.center)
                .padding(.top, 8)
            HStack(spacing: 8) {
                ForEach(Person.allCases) { person in
                    Button { onChosen(person) } label: {
                        VStack(spacing: 10) {
                            Circle().fill(person.color).frame(width: 76, height: 76)
                            Text(person.displayName).font(.szTitle).foregroundStyle(p.onBackground)
                        }
                        .padding(12)
                        .contentShape(RoundedRectangle(cornerRadius: 24))
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.top, 32)
            Spacer()
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(p.background.ignoresSafeArea())
    }
}

/// Create (first time) or enter the password. Also shown again if the password file was deleted on the server.
struct PasswordView: View {
    let person: Person
    let hasPassword: Bool
    let isBackup: Bool
    @EnvironmentObject var sync: SyncManager
    @Environment(\.palette) var p

    @State var password = ""
    @State var confirm = ""
    @State var error: String?
    @State var busy = false
    @FocusState private var focus: Field?

    private enum Field { case password, confirm }

    private var setup: Bool { !hasPassword }

    init(person: Person, hasPassword: Bool, isBackup: Bool) {
        self.person = person
        self.hasPassword = hasPassword
        self.isBackup = isBackup
    }

    private func submit() {
        if busy { return }
        if password.count < 4 { error = "Use at least 4 characters."; return }
        if setup && password != confirm { error = "Passwords don't match."; return }
        busy = true
        error = nil
        Task {
            let r = await sync.submitPassword(password, setup: setup)
            busy = false
            switch r {
            case .ok: error = nil
            case .wrong: error = "That password isn't right."
            case .tooShort: error = "Use at least 4 characters."
            case .alreadySet: error = "A password was just set from another device. Enter that one."
            case .notSet: error = "No password yet. Create one."
            case .unreachable: error = "Can't reach the server right now. Try again, or use offline for now."
            }
            if r == .alreadySet || r == .notSet { password = ""; confirm = "" }
        }
    }

    var body: some View {
        ScrollView {
            VStack(spacing: 0) {
                Circle().fill(person.color).frame(width: 64, height: 64).padding(.top, 72)
                Text(setup ? "Hi \(person.displayName)! Create a password" : "Welcome back, \(person.displayName)")
                    .font(.szHeadlineSmall).foregroundStyle(p.onBackground)
                    .multilineTextAlignment(.center)
                    .padding(.top, 16)
                Text((setup ? "You'll use it to sign in on any of your devices." : "Enter your password to sync this device.") +
                     (isBackup ? "\n\nYour main server can't be reached, so this is for the backup server." : ""))
                    .font(.szBody).foregroundStyle(p.onSurfaceVariant)
                    .multilineTextAlignment(.center)
                    .padding(.top, 8)

                field("Password", text: $password, focused: .password)
                    .padding(.top, 24)
                    .submitLabel(setup ? .next : .done)
                    .onSubmit { if setup { focus = .confirm } else { submit() } }
                if setup {
                    field("Confirm password", text: $confirm, focused: .confirm)
                        .padding(.top, 10)
                        .submitLabel(.done)
                        .onSubmit { submit() }
                }
                if let error {
                    Text(error).font(.szBody).foregroundStyle(p.error)
                        .multilineTextAlignment(.center)
                        .padding(.top, 10)
                }
                Button { submit() } label: {
                    Group {
                        if busy { ProgressView().tint(p.onPrimary) } else { Text(setup ? "Create password" : "Sign in") }
                    }
                    .frame(maxWidth: .infinity)
                }
                .buttonStyle(.szFilled)
                .disabled(busy)
                .padding(.top, 20)
                Button("Use offline for now") { sync.skipPassword() }.buttonStyle(.szText).padding(.top, 8)
                Button("Not \(person.displayName)? Switch person") { sync.signOut() }.buttonStyle(.szText)
            }
            .padding(24)
        }
        .scrollDismissesKeyboard(.interactively)
        .background(p.background.ignoresSafeArea())
        .onAppear { focus = .password }
    }

    private func field(_ label: String, text: Binding<String>, focused: Field) -> some View {
        SecureField(label, text: text)
            .textContentType(setup ? .newPassword : .password)
            .focused($focus, equals: focused)
            .font(.szBodyLarge)
            .foregroundStyle(p.onSurface)
            .padding(.horizontal, 16)
            .frame(height: 56)
            .background(p.surfaceContainer, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous)
                .strokeBorder(focus == focused ? p.primary : p.outline, lineWidth: focus == focused ? 2 : 1))
            .onChange(of: text.wrappedValue) { error = nil }
    }
}
