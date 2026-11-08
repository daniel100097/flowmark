class InvitationsController < ApplicationController
  def create
    team = Team.find(params[:team_id])
    authorize team, :invite?

    invitation = team.invitations.create!(
      email: invitation_params[:email].strip.downcase,
      invited_by: current_user,
      expires_at: 7.days.from_now
    )

    render json: { id: invitation.id, email: invitation.email, expires_at: invitation.expires_at }, status: :created
  rescue ActiveRecord::RecordNotUnique
    render json: { error: "already invited" }, status: :conflict
  end

  private

  def invitation_params
    params.require(:invitation).permit(:email)
  end
end
