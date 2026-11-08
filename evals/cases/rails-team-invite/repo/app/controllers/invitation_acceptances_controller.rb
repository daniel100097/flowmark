class InvitationAcceptancesController < ApplicationController
  def create
    invitation = Invitation.find_by!(token: params[:token])
    return head :gone if invitation.expired?
    return head :forbidden unless invitation.email.casecmp?(current_user.email)

    invitation.with_lock do
      unless invitation.accepted_at
        invitation.team.memberships.find_or_create_by!(user: current_user) { |m| m.role = "member" }
        invitation.update!(accepted_at: Time.current, accepted_by: current_user)
      end
    end

    render json: { team_id: invitation.team_id }, status: :ok
  end
end
