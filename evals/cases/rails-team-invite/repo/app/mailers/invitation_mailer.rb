class InvitationMailer < ApplicationMailer
  def invite
    @invitation = params[:invitation]
    @url = accept_invitation_url(token: @invitation.token)
    mail(to: @invitation.email, subject: "You're invited to #{@invitation.team.name}")
  end
end
