class Invitation < ApplicationRecord
  belongs_to :team
  belongs_to :invited_by, class_name: "User"
  belongs_to :accepted_by, class_name: "User", optional: true

  # Stored encrypted; deterministic so it can be looked up.
  encrypts :token, deterministic: true

  validates :email, presence: true, format: { with: URI::MailTo::EMAIL_REGEXP }

  before_create { self.token = SecureRandom.urlsafe_base64(32) }
  after_create_commit { InvitationMailer.with(invitation: self).invite.deliver_later }

  def expired?
    expires_at.past?
  end
end
